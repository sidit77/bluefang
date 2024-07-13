mod commands;
pub mod consts;
mod error;
// pub mod connection;
pub mod acl;
pub mod btsnoop;
pub mod connection;
mod event_loop;

use std::collections::BTreeSet;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::sync::OnceLock;
use std::time::Duration;

use bytes::{BufMut, Bytes, BytesMut};
pub use commands::*;
use instructor::utils::Length;
use instructor::{Buffer, BufferMut, Exstruct, LittleEndian};
use nusb::transfer::TransferError;
use parking_lot::Mutex;
use tokio::spawn;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender as MpscSender};
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tracing::{debug, error};

use crate::hci::acl::{AclHeader, BoundaryFlag, BroadcastFlag};
use crate::hci::consts::{EventCode, EventMask, Status};
use crate::hci::event_loop::{CmdResultSender, EventLoopCommand};
use crate::host::usb::UsbHost;
use crate::utils::Loggable;

//TODO make generic over transport
pub struct Hci {
    //transport: UsbHost,
    //router: Arc<EventRouter>,
    cmd_out: MpscSender<(Opcode, Bytes, CmdResultSender)>,
    acl_out: MpscSender<Bytes>,
    ctl_out: MpscSender<EventLoopCommand>,
    acl_size: usize,
    event_loop: Mutex<Option<JoinHandle<()>>>,
    version: LocalVersion
}

impl Hci {
    pub async fn new(transport: UsbHost) -> Result<Self, Error> {
        let (acl_out, acl_in) = unbounded_channel();
        let (cmd_out, cmd_in) = unbounded_channel();
        let (ctl_out, ctl_in) = unbounded_channel();
        let event_loop = spawn(event_loop::event_loop(transport, cmd_in, acl_in, ctl_in));
        let mut hci = Self {
            cmd_out,
            acl_out,
            ctl_out,
            acl_size: 0,
            event_loop: Mutex::new(Some(event_loop)),
            version: Default::default(),
        };

        // Reset after allowing the event loop to discard any unexpected events
        tokio::time::sleep(Duration::from_millis(100)).await;
        debug!("HCI reset...");
        hci.reset().await?;

        Self::try_load_firmware(&hci).await;

        hci.version = hci.read_local_version().await?;
        debug!("HCI version: {:?}", hci.version);

        //debug!("{:?}", hci.read_local_supported_commands().await?);

        hci.set_event_mask(EventMask::all()).await?;

        let buffer_size = hci.read_buffer_size().await?;
        hci.acl_size = buffer_size.acl_data_packet_length as usize;
        hci.ctl_out
            .send(EventLoopCommand::SetMaxInFlightAclPackets(buffer_size.total_num_acl_data_packets as u32))
            .map_err(|_| Error::EventLoopClosed)?;

        Ok(hci)
    }

    pub fn register_event_handler(&self, events: impl Into<BTreeSet<EventCode>>, handler: MpscSender<(EventCode, Bytes)>) -> Result<(), Error> {
        let events = events.into();
        debug_assert!(!events.is_empty());
        debug_assert!(!events.contains(&EventCode::CommandComplete));
        debug_assert!(!events.contains(&EventCode::CommandStatus));
        self.ctl_out
            .send(EventLoopCommand::RegisterHciEventHandler { events, handler })
            .map_err(|_| Error::EventLoopClosed)
    }

    pub fn register_data_handler(&self, handler: MpscSender<Bytes>) -> Result<(), Error> {
        self.ctl_out
            .send(EventLoopCommand::RegisterAclDataHandler { handler })
            .map_err(|_| Error::EventLoopClosed)
    }

    pub fn get_acl_sender(&self) -> AclSender {
        AclSender {
            sender: self.acl_out.clone(),
            max_size: self.acl_size
        }
    }

    pub async fn call<T: Exstruct<LittleEndian>>(&self, cmd: Opcode) -> Result<T, Error> {
        self.call_with_args(cmd, |_| {}).await
    }

    pub async fn call_with_args<T: Exstruct<LittleEndian>>(&self, cmd: Opcode, packer: impl FnOnce(&mut BytesMut)) -> Result<T, Error> {
        // TODO: check if the command is supported
        let mut buf = BytesMut::with_capacity(255);
        buf.write::<u16, LittleEndian>(cmd.into());
        buf.write::<u8, LittleEndian>(0);
        packer(&mut buf);
        let payload_len = u8::try_from(buf.len() - 3).map_err(|_| Error::PayloadTooLarge)?;
        buf[2] = payload_len;

        let (tx, rx) = tokio::sync::oneshot::channel();
        self.cmd_out
            .send((cmd, buf.freeze(), tx))
            .map_err(|_| Error::EventLoopClosed)?;
        //TODO: 1s timeout
        let mut resp = rx.await.map_err(|_| Error::EventLoopClosed)??;
        let status: Status = resp.read_le()?;
        match status {
            Status::Success => {
                let result: T = resp.read_le()?;
                resp.finish()?;
                Ok(result)
            }
            _ => Err(Error::Controller(status))
        }
    }

    pub async fn shutdown(&self) -> Result<(), Error> {
        let handle = self.event_loop.lock().take();
        if let Some(event_loop) = handle {
            self.reset().await?;
            self.ctl_out
                .send(EventLoopCommand::Shutdown)
                .map_err(|_| Error::EventLoopClosed)?;
            event_loop.await.unwrap();
            sleep(Duration::from_millis(100)).await;
        } else {
            error!("Another thread already called shutdown");
        }
        Ok(())
    }
}

impl Debug for Hci {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hci")
            .field("company", &self.version.company_id)
            .field("version", &self.version.hci_version)
            .finish()
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, thiserror::Error)]
pub enum AclSendError {
    #[error("The underlying event loop has been closed")]
    EventLoopClosed,
    #[error("Failed to build packet: {0}")]
    InvalidData(#[from] instructor::Error)
}

impl Loggable for AclSendError {
    fn should_log(&self) -> bool {
        matches!(self, AclSendError::InvalidData(_))
    }
}

#[derive(Clone)]
pub struct AclSender {
    sender: MpscSender<Bytes>,
    max_size: usize
}

impl AclSender {
    pub fn send(&self, handle: u16, pdu: Bytes) -> Result<(), AclSendError> {
        //trace!("Sending ACL data to handle 0x{:04X}", handle);
        let mut buffer = BytesMut::with_capacity(512);
        let mut pb = BoundaryFlag::FirstNonAutomaticallyFlushable;
        for chunk in pdu.chunks(self.max_size) {
            buffer.write(AclHeader {
                handle,
                pb,
                bc: BroadcastFlag::PointToPoint,
                length: Length::new(chunk.len())?
            });
            buffer.put(chunk);
            self.sender
                .send(buffer.split().freeze())
                .map_err(|_| AclSendError::EventLoopClosed)?;
            pb = BoundaryFlag::Continuing;
        }
        Ok(())
    }
}

//impl Drop for Hci {
//    fn drop(&mut self) {
//        self.event_loop.abort();
//    }
//}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("{0}")]
    Generic(&'static str),
    #[error(transparent)]
    TransportError(#[from] nusb::Error),
    #[error(transparent)]
    TransferError(#[from] TransferError),
    #[error("Payload exceeds maximum size (255)")]
    PayloadTooLarge,
    #[error("Malformed packet: {0:?}")]
    BadPacket(#[from] instructor::Error),
    #[error("Event loop closed")]
    EventLoopClosed,
    #[error("Unknown HCI Event code: 0x{0:02X}")]
    UnknownEventCode(u8),
    #[error("Unexpected HCI Command Response for {0:?}")]
    UnexpectedCommandResponse(Opcode),
    #[error("Unknown connection handle: 0x{0:02X}")]
    UnknownConnectionHandle(u16),
    #[error(transparent)]
    Controller(#[from] Status),
    #[error("Unknown channel id: 0x{0:02X}")]
    UnknownChannelId(u16)
}

impl Error {
    pub fn is_timeout(&self) -> bool {
        match self {
            Error::TransportError(err) => err.kind() == std::io::ErrorKind::TimedOut,
            _ => false
        }
    }
}

impl From<&'static str> for Error {
    fn from(value: &'static str) -> Self {
        Self::Generic(value)
    }
}

pub trait FirmwareLoader: Send + Sync {
    fn try_load_firmware<'a>(&'a self, hci: &'a Hci) -> Pin<Box<dyn Future<Output = Result<bool, Error>> + Send + 'a>>;

    fn boxed(self) -> Box<dyn FirmwareLoader> where Self: 'static + Sized {
        Box::new(self)
    }
}

static FIRMWARE_LOADERS: OnceLock<Vec<Box<dyn FirmwareLoader>>> = OnceLock::new();
impl Hci {
    pub fn register_firmware_loaders<I: IntoIterator<Item=Box<dyn FirmwareLoader>>>(loaders: I) {
        FIRMWARE_LOADERS.set(loaders.into_iter().collect())
            .unwrap_or_else(|_| panic!("Firmware loaders already registered"));
    }

    async fn try_load_firmware(&self) {
        if let Some(loaders) = FIRMWARE_LOADERS.get() {
            for loader in loaders {
                match loader.try_load_firmware(self).await {
                    Ok(true) => break,
                    Ok(false) => continue,
                    Err(err) => error!("Failed to load firmware: {:?}", err)
                }
            }
        }
    }
}
