pub mod signaling;
pub mod channel;

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::AtomicU8;
use bytes::Bytes;
use instructor::{Buffer, Exstruct, Instruct};
use instructor::utils::Length;
use tokio::{select, spawn};
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::mpsc::{UnboundedSender as MpscSender};
use tokio::task::JoinHandle;
use tracing::{debug, trace, warn};
use crate::ensure;
use crate::hci::acl::{AclDataAssembler, AclHeader};
use crate::hci::consts::{EventCode, LinkType, RemoteAddr, Status};
use crate::hci::{AclSender, Error, Hci};
use crate::l2cap::channel::Channel;

pub const SDP_PSM: u16 = 0x0001;
pub const AVCTP_PSM: u16 = 0x0017;
pub const AVDTP_PSM: u16 = 0x0019;



const CID_ID_NONE: u16 = 0x0000;
const CID_ID_SIGNALING: u16 = 0x0001;
const CID_RANGE_DYNAMIC: Range<u16> = 0x0040..0xFFFF;

#[derive(Default)]
pub struct L2capServerBuilder {
    servers: BTreeMap<u64, Box<dyn Server + Send>>,
}

impl L2capServerBuilder {

    pub fn with_server<P: Into<u64>, S: Server + Send + 'static>(mut self, psm: P, server: S) -> Self {
        self.servers.insert(psm.into(), Box::new(server));
        self
    }

    pub fn spawn(self, hci: Arc<Hci>) -> Result<JoinHandle<()>, Error> {
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
        let sender = hci.get_acl_sender();
        Ok(spawn(async move {
            let mut state = State {
                sender,
                connections: Default::default(),
                servers: self.servers,
                channels: Default::default(),
                next_signaling_id: Arc::new(Default::default()),
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
        }))
    }

}

#[allow(dead_code)]
struct PhysicalConnection {
    handle: u16,
    max_slots: u8,
    addr: RemoteAddr,
    assembler: AclDataAssembler,
}

struct State {
    sender: AclSender,
    connections: BTreeMap<u16, PhysicalConnection>,
    servers: BTreeMap<u64, Box<dyn Server + Send>>,
    channels: BTreeMap<u16, (u16, MpscSender<ChannelEvent>)>,
    next_signaling_id: Arc<AtomicU8>
}

impl State {

    fn get_connection(&mut self, handle: u16) -> Result<&mut PhysicalConnection, Error> {
        self.connections.get_mut(&handle).ok_or(Error::UnknownConnectionHandle(handle))
    }

    fn send_channel_msg(&self, cid: u16, msg: ChannelEvent) -> Result<(), Error> {
        let (_, channel) = self.channels.get(&cid).ok_or(Error::UnknownChannelId(cid))?;
        channel.send(msg).expect("Channel closed");
        Ok(())
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
        //trace!("Received {} bytes of ACL data", data.len());
        let header: AclHeader = data.read()?;
        if let Some(pdu) = self.get_connection(header.handle)?.assembler.push(header, data) {
            self.handle_l2cap_packet(header.handle, pdu)?;
        }
        Ok(())
    }

    // ([Vol 3] Part A, Section 3.1).
    fn handle_l2cap_packet(&mut self, handle: u16, mut data: Bytes) -> Result<(), Error> {
        let L2capHeader { cid, ..} = data.read()?;

        //debug!("    L2CAP header: cid={:04X}", cid);
        // ([Vol 3] Part A, Section 2.1).
        match cid {
            CID_ID_NONE => Err(Error::BadPacket(instructor::Error::InvalidValue)),
            CID_ID_SIGNALING => self.handle_l2cap_signaling(handle, data),
            cid if CID_RANGE_DYNAMIC.contains(&cid) => self.send_channel_msg(cid, ChannelEvent::DataReceived(data)),
            _ => {
                warn!("Unhandled L2CAP CID: {:04X}", cid);
                Ok(())
            },
        }
    }

    fn handle_channel_connection(&mut self, handle: u16, psm: u64, scid: u16) -> Result<u16, ConnectionResult> {
        debug!("        Connection request: PSM={:04X} SCID={:04X}", psm, scid);
        let server = self.servers.get_mut(&psm).ok_or(ConnectionResult::RefusedPsmNotSupported)?;
        //ensure!(self.servers.contains_key(&psm), ConnectionResult::RefusedPsmNotSupported);
        ensure!(CID_RANGE_DYNAMIC.contains(&scid), ConnectionResult::RefusedInvalidSourceCid);
        //TODO check if source cid already exists for physical connection

        let dcid = CID_RANGE_DYNAMIC
            .clone()
            .find(|&cid| !self.channels.contains_key(&cid) && cid != scid)
            .ok_or(ConnectionResult::RefusedNoResources)?;

        let (tx, rx) = unbounded_channel();
        let channel = Channel {
            connection_handle: handle,
            remote_cid: scid,
            local_cid: dcid,
            receiver: rx,
            sender: self.sender.clone(),
            next_signaling_id: self.next_signaling_id.clone(),
            local_mtu: 0,
            remote_mtu: 0,
        };
        self.channels.insert(dcid, (scid, tx));
        server.on_connection(channel);

        Ok(dcid)
    }

}

// ([Vol 3] Part A, Section 3.1).
#[derive(Debug, Exstruct, Instruct)]
#[instructor(endian = "little")]
pub struct L2capHeader {
    pub len: Length<u16, 2>,
    pub cid: u16
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Instruct)]
#[repr(u16)]
pub enum ConnectionResult {
    Success = 0x0000,
    Pending = 0x0001,
    RefusedPsmNotSupported = 0x0002,
    RefusedSecurityBlock = 0x0003,
    RefusedNoResources = 0x0004,
    RefusedInvalidSourceCid = 0x0006,
    RefusedSourceCidAlreadyAllocated = 0x0007,
}

#[derive(Default, Debug, Copy, Clone, Eq, PartialEq, Instruct)]
#[repr(u16)]
pub enum ConnectionStatus {
    #[default]
    NoFurtherInformation = 0x0000,
    AuthenticationPending = 0x0001,
    AuthorizationPending = 0x0002,
}

#[allow(dead_code)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
#[repr(u16)]
pub enum ConfigureResult {
    Success = 0x0000,
    UnacceptableParameters = 0x0001,
    Rejected = 0x0002,
    UnknownOptions = 0x0003,
    Pending = 0x0004,
    FlowSpecRejected = 0x0005,
}

pub enum ChannelEvent {
    DataReceived(Bytes),
    ConfigurationRequest(u8, Bytes),
    ConfigurationResponse(u8, ConfigureResult, Bytes),
}



pub trait Server {

    fn on_connection(&mut self, channel: Channel);

}

