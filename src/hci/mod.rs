pub mod consts;
mod error;
mod commands;
// pub mod connection;
pub mod acl;
mod event_loop;
pub mod connection;
pub mod btsnoop;

use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use bytes::{BufMut, Bytes, BytesMut};
use instructor::{Buffer, BufferMut, Exstruct, LittleEndian};
use instructor::utils::Length;
use nusb::transfer::TransferError;
use parking_lot::Mutex;
use tokio::spawn;
use tokio::task::JoinHandle;
use tracing::{debug, error, trace};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender as MpscSender};
use tokio::time::sleep;
use crate::host::usb::UsbHost;
use crate::hci::consts::{EventCode, EventMask, Status};

pub use commands::*;
use crate::hci::acl::{AclHeader, BoundaryFlag, BroadcastFlag};
use crate::hci::event_loop::{CmdResultSender, EventLoopCommand};


//TODO make generic over transport
pub struct Hci {
    //transport: UsbHost,
    //router: Arc<EventRouter>,
    cmd_out: MpscSender<(Opcode, Bytes, CmdResultSender)>,
    acl_out: MpscSender<Bytes>,
    ctl_out: MpscSender<EventLoopCommand>,
    acl_size: usize,
    event_loop: Mutex<Option<JoinHandle<()>>>
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
            event_loop: Mutex::new(Some(event_loop))
        };

        // Reset after allowing the event loop to discard any unexpected events
        tokio::time::sleep(Duration::from_millis(100)).await;
        debug!("HCI reset...");
        hci.reset().await?;

        Self::try_load_firmware(&hci).await;

        debug!("HCI version: {:?}", hci.read_local_version().await?);

        debug!("{:?}", hci.read_local_supported_commands().await?);

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
        self.ctl_out.send(EventLoopCommand::RegisterHciEventHandler {
            events,
            handler
        }).map_err(|_| Error::EventLoopClosed)
    }

    pub fn register_data_handler(&self, handler: MpscSender<Bytes>) -> Result<(), Error> {
        self.ctl_out.send(EventLoopCommand::RegisterAclDataHandler {
            handler
        }).map_err(|_| Error::EventLoopClosed)
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
        buf.write::<u16, LittleEndian>(&cmd.into());
        buf.write::<u8, LittleEndian>(&0);
        packer(&mut buf);
        let payload_len = u8::try_from(buf.len() - 3).map_err(|_| Error::PayloadTooLarge)?;
        buf[2] = payload_len;

        let (tx, rx) = tokio::sync::oneshot::channel();
        self.cmd_out.send((cmd, buf.freeze(), tx)).map_err(|_| Error::EventLoopClosed)?;
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
            self.ctl_out.send(EventLoopCommand::Shutdown).map_err(|_| Error::EventLoopClosed)?;
            event_loop.await.unwrap();
            sleep(Duration::from_millis(100)).await;
        } else {
            error!("Another thread already called shutdown");
        }
        Ok(())
    }

}
#[derive(Clone)]
pub struct AclSender {
    sender: MpscSender<Bytes>,
    max_size: usize
}

impl AclSender {
    pub fn send(&self, handle: u16, pdu: Bytes) -> Result<(), Error> {
        //trace!("Sending ACL data to handle 0x{:04X}", handle);
        let mut buffer = BytesMut::with_capacity(512);
        let mut pb = BoundaryFlag::FirstNonAutomaticallyFlushable;
        for chunk in pdu.chunks(self.max_size) {
            buffer.write(&AclHeader {
                handle,
                pb,
                bc: BroadcastFlag::PointToPoint,
                length: Length::new(chunk.len())?
            });
            buffer.put(chunk);
            self.sender.send(buffer.split().freeze()).map_err(|_| Error::EventLoopClosed)?;
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
    UnknownChannelId(u16),
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

pub trait FirmwareLoader {
    fn try_load_firmware<'a>(&'a self, hci: &'a Hci) -> Pin<Box<dyn Future<Output=Result<bool, Error>> + Send + 'a>>;
}

static FIRMWARE_LOADERS: tokio::sync::Mutex<Vec<Box<dyn FirmwareLoader + Send>>> = tokio::sync::Mutex::const_new(Vec::new());
impl Hci {

    pub async fn register_firmware_loader<FL: FirmwareLoader + Send + 'static>(loader: FL) {
        FIRMWARE_LOADERS.lock().await.push(Box::new(loader));
    }

    async fn try_load_firmware(&self) {
        for loader in &*FIRMWARE_LOADERS.lock().await {
            match loader.try_load_firmware(self).await {
                Ok(true) => break,
                Ok(false) => continue,
                Err(err) => error!("Failed to load firmware: {:?}", err)
            }
        }
    }

}

