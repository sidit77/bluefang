mod signaling;

use std::collections::BTreeMap;
use std::sync::Arc;
use bytes::Bytes;
use instructor::{Buffer, Exstruct, Instruct};
use instructor::utils::Length;
use smallvec::SmallVec;
use tokio::{select, spawn};
use tokio::sync::mpsc::unbounded_channel;
use tracing::{debug, trace, warn};
use crate::hci::acl::{AclDataAssembler, AclHeader};
use crate::hci::consts::{EventCode, LinkType, RemoteAddr, Status};
use crate::hci::{Error, Hci};

const CID_ID_NONE: u16 = 0x0000;
const CID_ID_SIGNALING: u16 = 0x0001;

pub fn start_l2cap_server(hci: Arc<Hci>) -> Result<(), Error> {
    let mut data = {
        let (tx, rx) = unbounded_channel();
        hci.register_data_handler(tx)?;
        rx
    };
    let mut events = {
        let (tx, rx) = unbounded_channel();
        hci.register_event_handler(
            [
                EventCode::ConnectionComplete,
                EventCode::DisconnectionComplete,
                EventCode::MaxSlotsChange
            ],
            tx)?;
        rx
    };
    spawn(async move {
        let mut state = State {
            hci,
            connections: Default::default(),
        };

        loop {
            select! {
                Some(event) = events.recv() => {
                    if let Err(err) = state.handle_event(event) {
                        warn!("Error handling event: {:?}", err);
                    }
                },
                Some(data) = data.recv() => {
                    if let Err(err) = state.handle_data(data) {
                        warn!("Error handling data: {:?}", err);
                    }
                },
                else => break,

            }
        }
        trace!("L2CAP server finished");
    });
    Ok(())
}

struct PhysicalConnection {
    handle: u16,
    max_slots: u8,
    addr: RemoteAddr,
    assembler: AclDataAssembler,
}

struct State {
    hci: Arc<Hci>,
    connections: BTreeMap<u16, PhysicalConnection>,
}

impl State {

    fn get_connection(&mut self, handle: u16) -> Result<&mut PhysicalConnection, Error> {
        self.connections.get_mut(&handle).ok_or(Error::UnknownConnectionHandle(handle))
    }

    fn handle_event(&mut self, (code, mut data): (EventCode, Bytes)) -> Result<(), Error> {
        match code {
            EventCode::ConnectionComplete => {
                // ([Vol 4] Part E, Section 7.7.3).
                let status: Status = data.read_le()?;
                let handle: u16 = data.read_le()?;
                let addr: RemoteAddr = data.read_le()?;
                let link_type: LinkType = data.read_le()?;
                let _encryption_enabled = data.read_le::<u8>().map(|b| b == 0x01)?;
                data.finish()?;

                assert_eq!(link_type, LinkType::Acl);
                if status == Status::Success {
                    assert!(self
                        .connections
                        .insert(handle, PhysicalConnection {
                            handle,
                            max_slots: 0x01,
                            addr,
                            assembler: AclDataAssembler::default(),
                        }).is_none());
                    debug!("Connection complete: 0x{:04X} {}", handle, addr);
                } else {
                    warn!("Connection failed: {:?}", status);
                }
            },
            EventCode::DisconnectionComplete => {
                // ([Vol 4] Part E, Section 7.7.5).
                let status: Status = data.read_le()?;
                let handle: u16 = data.read_le()?;
                let reason: Status = data.read_le()?;
                data.finish()?;

                self.connections.remove(&handle);
                if status == Status::Success {
                    debug!("Disconnection complete: {:?} {:?}", handle, reason);
                } else {
                    warn!("Disconnection failed: {:?}", status);
                }
            },
            EventCode::MaxSlotsChange => {
                // ([Vol 4] Part E, Section 7.7.27).
                let handle: u16 = data.read_le()?;
                let max_slots: u8 = data.read_le()?;
                data.finish()?;
                self.get_connection(handle)?.max_slots = max_slots;
                debug!("Max slots change: {:?} {:?}", handle, max_slots);
            }
            _ => unreachable!()
        }
        Ok(())
    }

    fn handle_data(&mut self, mut data: Bytes) -> Result<(), Error> {
        debug!("ACL data: {:02X?}", data);
        let header: AclHeader = data.read()?;
        if let Some(pdu) = self.get_connection(header.handle)?.assembler.push(header, data) {
            self.handle_l2cap_packet(header.handle, pdu)?;
        }
        Ok(())
    }

    // ([Vol 3] Part A, Section 3.1).
    fn handle_l2cap_packet(&mut self, handle: u16, mut data: Bytes) -> Result<(), Error> {
        let L2capHeader { cid, ..} = data.read()?;

        debug!("    L2CAP header: cid={:04X}", cid);
        // ([Vol 3] Part A, Section 2.1).
        match cid {
            CID_ID_NONE => Err(Error::BadPacket(instructor::Error::InvalidValue)),
            CID_ID_SIGNALING => self.handle_l2cap_signaling(handle, data),
            _ => {
                warn!("Unhandled L2CAP CID: {:04X}", cid);
                Ok(())
            },
        }
    }



}

// ([Vol 3] Part A, Section 3.1).
#[derive(Debug, Exstruct, Instruct)]
#[instructor(endian = "little")]
struct L2capHeader {
    len: Length<u16, 2>,
    cid: u16
}



#[derive(Default, Debug)]
pub struct ReplyPacket(SmallVec<[u8; 32]>);

impl ReplyPacket {

    pub fn prepend<const N: usize>(&mut self, data: [u8; N]) {
        self.0.insert_many(0, data);
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl AsRef<[u8]> for ReplyPacket {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

