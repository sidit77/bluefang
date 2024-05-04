mod signaling;

use std::collections::BTreeMap;
use std::sync::Arc;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use smallvec::SmallVec;
use tokio::{select, spawn};
use tokio::sync::mpsc::unbounded_channel;
use tracing::{debug, error, trace, warn};
use crate::{ensure, hci};
use crate::hci::acl::{AclDataAssembler, AclDataPacket};
use crate::hci::consts::{EventCode, LinkType, RemoteAddr, Status};
use crate::hci::{Error, Event, Hci};
use crate::utils::SliceExt;

const CID_ID_NONE: u16 = 0x0000;
const CID_ID_SIGNALING: u16 = 0x0001;

pub fn start_l2cap_server(hci: Arc<Hci>) -> Result<(), hci::Error> {
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

    fn handle_event(&mut self, event: Event) -> Result<(), Error> {
        let Event { code, mut data, .. } = event;
        match code {
            EventCode::ConnectionComplete => {
                // ([Vol 4] Part E, Section 7.7.3).
                let status = data.u8().map(Status::from)?;
                let handle = data.u16()?;
                let addr = data.array().map(RemoteAddr::from)?;
                let link_type = data.u8().map(LinkType::from)?;
                let _encryption_enabled = data.u8().map(|b| b == 0x01)?;
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
                let status = data.u8().map(Status::from)?;
                let handle = data.u16()?;
                let reason = data.u8().map(Status::from)?;
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
                let handle = data.u16()?;
                let max_slots = data.u8()?;
                data.finish()?;
                self.get_connection(handle)?.max_slots = max_slots;
                debug!("Max slots change: {:?} {:?}", handle, max_slots);
            }
            _ => unreachable!()
        }
        Ok(())
    }

    fn handle_data(&mut self, data: AclDataPacket) -> Result<(), Error> {
        debug!("ACL data: {:02X?}", data);
        let handle = data.handle;
        if let Some(pdu) = self.get_connection(handle)?.assembler.push(data) {
            self.handle_l2cap_packet(handle, &pdu)?;
        }
        Ok(())
    }

    // ([Vol 3] Part A, Section 3.1).
    fn handle_l2cap_packet(&mut self, handle: u16, data: &[u8]) -> Result<(), Error> {
        let len = u16::from_le_bytes(*data.get_chunk(0).ok_or(Error::BadEventPacketSize)?);
        let cid = u16::from_le_bytes(*data.get_chunk(2).ok_or(Error::BadEventPacketSize)?);
        let data = &data[4..];
        ensure!(data.len() == len as usize, Error::BadEventPacketSize);

        debug!("    L2CAP header: cid={:04X}", cid);
        // ([Vol 3] Part A, Section 2.1).
        match cid {
            CID_ID_NONE => Err(Error::BadEventPacketValue),
            CID_ID_SIGNALING => self.handle_l2cap_signaling(handle, data),
            _ => {
                warn!("Unhandled L2CAP CID: {:04X}", cid);
                Ok(())
            },
        }
    }



}


// ([Vol 3] Part A, Section 4).
#[derive(Debug, Copy, Clone, Eq, PartialEq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
enum SignalingCodes {
    CommandReject = 0x01,
    ConnectionRequest = 0x02,
    ConnectionResponse = 0x03,
    ConfigureRequest = 0x04,
    ConfigureResponse = 0x05,
    DisconnectionRequest = 0x06,
    DisconnectionResponse = 0x07,
    EchoRequest = 0x08,
    EchoResponse = 0x09,
    InformationRequest = 0x0A,
    InformationResponse = 0x0B,
    ConnectionParameterUpdateRequest = 0x12,
    ConnectionParameterUpdateResponse = 0x13,
    LECreditBasedConnectionRequest = 0x14,
    LECreditBasedConnectionResponse = 0x15,
    FlowControlCreditIndex = 0x16,
    CreditBasedConnectionRequest = 0x17,
    CreditBasedConnectionResponse = 0x18,
    CreditBasedReconfigurationRequest = 0x19,
    CreditBasedReconfigurationResponse = 0x1A,
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

