pub mod channel;
pub mod configuration;
pub mod signaling;

use std::collections::BTreeMap;
use std::future::Future;
use std::ops::Range;
use std::pin::Pin;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use instructor::utils::Length;
use instructor::{Buffer, Exstruct, Instruct};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender as MpscSender};
use tracing::{debug, warn};

use crate::hci::acl::{AclDataAssembler, AclHeader};
use crate::hci::consts::{ConnectionMode, EventCode, LinkType, RemoteAddr, Status};
use crate::hci::{AclSender, Error, Hci};
use crate::l2cap::channel::Channel;
use crate::l2cap::configuration::ConfigurationParameter;

pub const SDP_PSM: u16 = 0x0001;
pub const AVCTP_PSM: u16 = 0x0017;
pub const AVDTP_PSM: u16 = 0x0019;

const CID_ID_NONE: u16 = 0x0000;
const CID_ID_SIGNALING: u16 = 0x0001;
const CID_RANGE_DYNAMIC: Range<u16> = 0x0040..0xFFFF;

#[derive(Default)]
pub struct L2capServerBuilder {
    handlers: BTreeMap<u64, Box<dyn ProtocolHandler>>
}

impl L2capServerBuilder {
    pub fn with_protocol<P: ProtocolHandlerProvider>(mut self, provider: P) -> Self {
        for handler in provider.protocol_handlers() {
            assert!(self.handlers.insert(handler.psm(), handler).is_none(), "Duplicate PSMs");
        }
        self
    }

    pub fn run(self, hci: &Hci) -> Result<L2capServer, Error> {
        let data = {
            let (tx, rx) = unbounded_channel();
            hci.register_data_handler(tx)?;
            rx
        };
        let events = {
            let (tx, rx) = unbounded_channel();
            hci.register_event_handler(
                [EventCode::ConnectionComplete, EventCode::DisconnectionComplete, EventCode::MaxSlotsChange, EventCode::ModeChange],
                tx
            )?;
            rx
        };
        let sender = hci.get_acl_sender();
        Ok(L2capServer {
            data,
            events,
            sender,
            connections: Default::default(),
            handlers: self.handlers,
            channels: Default::default(),
            next_signaling_id: Default::default(),
        })
    }

}


#[allow(dead_code)]
struct PhysicalConnection {
    handle: u16,
    max_slots: u8,
    mode: ConnectionMode,
    addr: RemoteAddr,
    assembler: AclDataAssembler
}

#[must_use = "Futures do nothing unless you `.await` or poll them"]
pub struct L2capServer {
    data: UnboundedReceiver<Bytes>,
    events: UnboundedReceiver<(EventCode, Bytes)>,

    sender: AclSender,
    connections: BTreeMap<u16, PhysicalConnection>,
    handlers: BTreeMap<u64, Box<dyn ProtocolHandler>>,
    channels: BTreeMap<u16, MpscSender<ChannelEvent>>,
    next_signaling_id: SignalingIds
}

impl Future for L2capServer {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        while let Poll::Ready(data) = self.data.poll_recv(cx) {
            let Some(data) = data else { return Poll::Ready(()); };
            self.handle_data(data)
                .unwrap_or_else(|err| warn!("Error handling data: {:?}", err));
        }
        while let Poll::Ready(event) = self.events.poll_recv(cx) {
           let Some(event) = event else { return Poll::Ready(()); };
           self.handle_event(event)
               .unwrap_or_else(|err| warn!("Error handling event: {:?}", err));
        }
        Poll::Pending
    }
}

impl L2capServer {
    fn get_connection(&mut self, handle: u16) -> Result<&mut PhysicalConnection, Error> {
        self.connections
            .get_mut(&handle)
            .ok_or(Error::UnknownConnectionHandle(handle))
    }

    fn send_channel_msg(&mut self, cid: u16, msg: ChannelEvent) -> Result<(), Error> {
        let channel = self
            .channels
            .get(&cid)
            .ok_or(Error::UnknownChannelId(cid))?;
        if channel.send(msg).is_err() {
            warn!("Channel closed: {:?}", cid);
            self.channels.remove(&cid);
        }
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
                    assert!(
                        self.connections
                            .insert(
                                handle,
                                PhysicalConnection {
                                    handle,
                                    max_slots: 0x01,
                                    mode: ConnectionMode::default(),
                                    addr,
                                    assembler: AclDataAssembler::default()
                                }
                            )
                            .is_none()
                    );
                    debug!("Connection complete: 0x{:04X} {}", handle, addr);
                } else {
                    warn!("Connection failed: {:?}", status);
                }
            }
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
            }
            EventCode::MaxSlotsChange => {
                // ([Vol 4] Part E, Section 7.7.27).
                let handle: u16 = data.read_le()?;
                let max_slots: u8 = data.read_le()?;
                data.finish()?;
                self.get_connection(handle)?.max_slots = max_slots;
                debug!("Max slots changed for {:#04x}: {:?}", handle, max_slots);
            }
            EventCode::ModeChange => {
                // ([Vol 4] Part E, Section 7.7.20).
                let _status: Status = data.read_le()?;
                let handle: u16 = data.read_le()?;
                let current_mode: ConnectionMode = data.read_le()?;
                let _interval: u16 = data.read_le()?;
                data.finish()?;
                self.get_connection(handle)?.mode = current_mode;
                debug!("Mode change for {:#04x}: {:?}", handle, current_mode);
            }
            _ => unreachable!()
        }
        Ok(())
    }

    fn handle_data(&mut self, mut data: Bytes) -> Result<(), Error> {
        //trace!("Received {} bytes of ACL data", data.len());
        let header: AclHeader = data.read()?;
        if let Some(pdu) = self
            .get_connection(header.handle)?
            .assembler
            .push(header, data)
        {
            self.handle_l2cap_packet(header.handle, pdu)?;
        }
        Ok(())
    }

    // ([Vol 3] Part A, Section 3.1).
    fn handle_l2cap_packet(&mut self, handle: u16, mut data: Bytes) -> Result<(), Error> {
        let L2capHeader { cid, .. } = data.read()?;

        //debug!("    L2CAP header: cid={:04X}", cid);
        // ([Vol 3] Part A, Section 2.1).
        match cid {
            CID_ID_NONE => Err(Error::BadPacket(instructor::Error::InvalidValue)),
            CID_ID_SIGNALING => self.handle_l2cap_signaling(handle, data),
            cid if CID_RANGE_DYNAMIC.contains(&cid) => self.send_channel_msg(cid, ChannelEvent::DataReceived(data)),
            _ => {
                warn!("Unhandled L2CAP CID: {:04X}", cid);
                Ok(())
            }
        }
    }

    pub fn new_channel(&mut self, handle: u16) -> Option<Channel> {
        assert!(self.connections.contains_key(&handle));
        self.channels.retain(|_, tx| !tx.is_closed());
        let scid = CID_RANGE_DYNAMIC
            .clone()
            .find(|&cid| !self.channels.contains_key(&cid))?;
        let (tx, rx) = unbounded_channel();
        self.channels.insert(scid, tx);
        let channel = Channel::new(
            handle,
            scid,
            rx,
            self.sender.clone(),
            self.next_signaling_id.clone()
        );
        Some(channel)
    }
}

// ([Vol 3] Part A, Section 3.1).
#[derive(Debug, Exstruct, Instruct)]
#[instructor(endian = "little")]
pub struct L2capHeader {
    pub len: Length<u16, 2>,
    pub cid: u16
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
#[repr(u16)]
pub enum ConnectionResult {
    Success = 0x0000,
    Pending = 0x0001,
    RefusedPsmNotSupported = 0x0002,
    RefusedSecurityBlock = 0x0003,
    RefusedNoResources = 0x0004,
    RefusedInvalidSourceCid = 0x0006,
    RefusedSourceCidAlreadyAllocated = 0x0007
}

#[derive(Default, Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
#[repr(u16)]
pub enum ConnectionStatus {
    #[default]
    NoFurtherInformation = 0x0000,
    AuthenticationPending = 0x0001,
    AuthorizationPending = 0x0002
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
    FlowSpecRejected = 0x0005
}

#[derive(Clone)]
pub struct SignalingIds(Arc<AtomicU8>);

impl Default for SignalingIds {
    fn default() -> Self {
        Self(Arc::new(AtomicU8::new(1)))
    }
}

impl SignalingIds {
    pub fn next(&self) -> u8 {
        let mut current = self.0.load(Ordering::Relaxed);
        loop {
            let next = current.checked_add(1).unwrap_or(1);
            match self
                .0
                .compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed)
            {
                Ok(_) => break,
                Err(i) => current = i
            }
        }
        current
    }
}

pub enum ChannelEvent {
    DataReceived(Bytes),
    ConnectionResponse {
        id: u8,
        remote_cid: u16,
        result: ConnectionResult,
        status: ConnectionStatus
    },
    ConfigurationRequest {
        id: u8,
        options: Vec<ConfigurationParameter>
    },
    ConfigurationResponse {
        id: u8,
        result: ConfigureResult,
        options: Vec<ConfigurationParameter>
    },
    DisconnectRequest {
        id: u8
    },
    DisconnectResponse {
        id: u8
    }
}

pub trait ProtocolHandlerProvider {
    fn protocol_handlers(&self) -> Vec<Box<dyn ProtocolHandler>>;
}

pub trait ProtocolHandler: Send {
    fn psm(&self) -> u64;

    fn handle(&self, channel: Channel);
}

impl<P> ProtocolHandlerProvider for P
where
    P: ProtocolHandler + Clone + 'static
{
    fn protocol_handlers(&self) -> Vec<Box<dyn ProtocolHandler>> {
        vec![Box::new(self.clone())]
    }
}

pub struct ProtocolDelegate<H, F> {
    psm: u64,
    handler: H,
    map_func: F
}

impl<H, F> ProtocolDelegate<H, F>
where
    H: Send + 'static,
    F: Fn(&H, Channel) + Send + 'static
{
    pub fn boxed<I: Into<u64>>(psm: I, handler: H, map_func: F) -> Box<dyn ProtocolHandler> {
        Box::new(Self {
            psm: psm.into(),
            handler,
            map_func
        })
    }
}

impl<H, F> ProtocolHandler for ProtocolDelegate<H, F>
where
    H: Send,
    F: Fn(&H, Channel) + Send
{
    fn psm(&self) -> u64 {
        self.psm
    }

    fn handle(&self, channel: Channel) {
        (self.map_func)(&self.handler, channel)
    }
}
