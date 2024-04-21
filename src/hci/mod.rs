pub mod consts;
mod error;
mod buffer;

use std::mem::size_of;
use nusb::transfer::{ControlOut, ControlType, Recipient, RequestBuffer};
use tokio::spawn;
use tokio::task::JoinHandle;
use crate::hci::consts::Opcode;
use crate::host::usb::UsbHost;
use crate::hci::buffer::SendBuffer;

const MAX_HCI_EVENT_SIZE: usize = 1 + size_of::<u8>() + u8::MAX as usize;
const HCI_EVENT_QUEUE_SIZE: usize = 4;

//TODO make generic over transport
pub struct Host {
    transport: UsbHost,
    event_loop: JoinHandle<()>
}

impl Host {
    pub fn new(transport: UsbHost) -> Self {
        let mut events = transport.interface.interrupt_in_queue(transport.endpoints.event);
        let event_loop = spawn(async move {
            for _ in 0..HCI_EVENT_QUEUE_SIZE {
                events.submit(RequestBuffer::new(MAX_HCI_EVENT_SIZE));
            }
            loop {
                let event = events.next_complete().await;
                event.status.unwrap();
                println!("{:?}", event.data);
                events.submit(RequestBuffer::reuse(event.data, MAX_HCI_EVENT_SIZE));
            }
        });
        Host {
            transport,
            event_loop,
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
}
