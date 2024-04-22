pub mod consts;
mod error;
mod buffer;
mod events;
mod commands;

use std::future::Future;
use std::mem::size_of;
use std::sync::Arc;
use std::time::Duration;
use nusb::transfer::{ControlOut, ControlType, Recipient, RequestBuffer};
use tokio::spawn;
use tokio::task::JoinHandle;
use tracing::{debug, error};
use crate::ensure;
use crate::host::usb::UsbHost;
use crate::hci::buffer::SendBuffer;
use crate::hci::events::{EventRouter, FromEvent};
use crate::hci::consts::Status;

pub use commands::*;


const MAX_HCI_EVENT_SIZE: usize = 1 + size_of::<u8>() + u8::MAX as usize;
const HCI_EVENT_QUEUE_SIZE: usize = 4;

//TODO make generic over transport
pub struct Host {
    transport: UsbHost,
    router: Arc<EventRouter>,
    event_loop: JoinHandle<()>
}

impl Host {
    pub async fn new(transport: UsbHost) -> Result<Self, Error> {
        let router = Arc::new(EventRouter::default());
        let event_loop = spawn(Self::event_loop(&transport, router.clone()));
        let host = Host {
            transport,
            router,
            event_loop,
        };

        // Reset after allowing the event loop to discard any unexpected events
        tokio::time::sleep(Duration::from_millis(100)).await;
        debug!("HCI reset...");
        host.reset().await?;

        debug!("HCI version: {:?}", host.read_local_version().await?);

        debug!("{:?}", host.read_local_supported_commands().await?);

        Ok(host)
    }

    fn event_loop(transport: &UsbHost, router: Arc<EventRouter>) -> impl Future<Output=()> {
        let mut events = transport.interface.interrupt_in_queue(transport.endpoints.event);
        for _ in 0..HCI_EVENT_QUEUE_SIZE {
            events.submit(RequestBuffer::new(MAX_HCI_EVENT_SIZE));
        }
        async move {
            loop {
                let event = events.next_complete().await;
                match event.status {
                    Ok(_) => router
                        .handle_event(&event.data)
                        .unwrap_or_else(|err| error!("Error handling event: {:?} ({:?})", err, event.data)),
                    Err(err) => error!("Error reading HCI event: {:?}", err),
                }
                events.submit(RequestBuffer::reuse(event.data, MAX_HCI_EVENT_SIZE));
            }
        }
    }

    pub async fn call<T: FromEvent>(&self, cmd: Opcode) -> Result<T, Error> {
        self.call_with_args(cmd, |_| {}).await
    }

    pub async fn call_with_args<T: FromEvent>(&self, cmd: Opcode, packer: impl FnOnce(&mut SendBuffer)) -> Result<T, Error> {
        // TODO: check if the command is supported
        let mut buf = SendBuffer::default();
        buf.put_u16(cmd);
        // we'll update this later
        buf.put_u8(0);
        packer(&mut buf);
        let payload_len = u8::try_from(buf.len() - 3).map_err(|_| Error::PayloadTooLarge)?;
        buf.set_u8(2, payload_len);

        let rx = self.router.reserve(cmd).await;

        let cmd = self.transport.interface.control_out(ControlOut {
            control_type: ControlType::Class,
            recipient: Recipient::Interface,
            request: 0x00,
            value: 0x00,
            index: self.transport.endpoints.main_iface.into(),
            data: buf.data(),
        }).await;
        cmd.status?;

        let mut resp = rx.await.expect("Message handler dropped");
        let status = Status::from(resp.get_u8().ok_or(Error::BadEventPacketSize)?);
        match status {
            Status::Success => {
                let result = T::unpack(&mut resp).ok_or(Error::BadEventPacketSize)?;
                ensure!(resp.remaining() == 0, Error::BadEventPacketSize);
                Ok(result)
            }
            _ => Err(Error::Controller(status))
        }
    }

}

impl Drop for Host {
    fn drop(&mut self) {
        self.event_loop.abort();
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error(transparent)]
    TransportError(#[from] nusb::Error),
    #[error(transparent)]
    TransferError(#[from] nusb::transfer::TransferError),
    #[error("Payload exceeds maximum size (255)")]
    PayloadTooLarge,
    #[error("HCI Event has an invalid size")]
    BadEventPacketSize,
    #[error("Unkown HCI Event code: 0x{0:02X}")]
    UnknownEventCode(u8),
    #[error("Unexpected HCI Command Response for {0:?}")]
    UnexpectedCommandResponse(Opcode),
    #[error(transparent)]
    Controller(#[from] Status)
}

impl Error {
    pub fn is_timeout(&self) -> bool {
        match self {
            Error::TransportError(err) => err.kind() == std::io::ErrorKind::TimedOut,
            _ => false
        }
    }
}