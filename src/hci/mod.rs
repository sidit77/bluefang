pub mod consts;
mod error;
mod buffer;
mod events;

use std::future::Future;
use std::mem::size_of;
use std::sync::Arc;
use nusb::transfer::{ControlOut, ControlType, Recipient, RequestBuffer, TransferError};
use smallvec::SmallVec;
use tokio::spawn;
use tokio::task::JoinHandle;
use tracing::{debug, error};
use crate::ensure;
use crate::hci::consts::{EventCode, Opcode, Status};
use crate::host::usb::UsbHost;
use crate::hci::buffer::SendBuffer;
use crate::hci::events::{EventRouter};

const MAX_HCI_EVENT_SIZE: usize = 1 + size_of::<u8>() + u8::MAX as usize;
const HCI_EVENT_QUEUE_SIZE: usize = 4;

//TODO make generic over transport
pub struct Host {
    transport: UsbHost,
    event_loop: JoinHandle<()>
}

impl Host {
    pub fn new(transport: UsbHost) -> Self {

        let event_loop = spawn(Self::event_loop(&transport));
        Host {
            transport,
            event_loop,
        }
    }

    fn event_loop(transport: &UsbHost) -> impl Future<Output=()> {
        let mut events = transport.interface.interrupt_in_queue(transport.endpoints.event);
        for _ in 0..HCI_EVENT_QUEUE_SIZE {
            events.submit(RequestBuffer::new(MAX_HCI_EVENT_SIZE));
        }
        let router = Arc::new(EventRouter::default());
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

    pub async fn call(&self, cmd: Opcode) -> Result<(), Error> {
        self.call_with_args(cmd, |buf| {}).await
    }

    pub async fn call_with_args<F: FnOnce(&mut SendBuffer)>(&self, cmd: Opcode, packer: F) -> Result<(), Error> {
        let mut buf = SendBuffer::default();
        buf.put_u16(cmd);
        // we'll update this later
        buf.put_u8(0);
        packer(&mut buf);
        let payload_len = u8::try_from(buf.len() - 3).map_err(|_| Error::PayloadTooLarge)?;
        buf.set_u8(2, payload_len);

        let cmd = self.transport.interface.control_out(ControlOut {
            control_type: ControlType::Class,
            recipient: Recipient::Interface,
            request: 0x00,
            value: 0x00,
            index: self.transport.endpoints.main_iface.into(),
            data: buf.data(),
        }).await;
        cmd.status?;
        Ok(())
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
}
