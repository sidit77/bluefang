use std::collections::VecDeque;
use parking_lot::Mutex;
use tokio::sync::oneshot::{Receiver, Sender};
use tracing::{debug, trace};
use crate::ensure;
use crate::hci::buffer::ReceiveBuffer;
use crate::hci::consts::{EventCode, Opcode};
use crate::hci::Error;

#[derive(Default)]
pub struct EventRouter {
    commands: Mutex<VecDeque<(Opcode, Sender<ReceiveBuffer>)>>
}

impl EventRouter {

    pub async fn reserve(&self, opcode: Opcode) -> Receiver<ReceiveBuffer> {
        // TODO implement command quota
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.commands.lock().push_back((opcode, tx));
        rx
    }

    pub fn handle_event(&self, data: &[u8]) -> Result<(), Error> {
        let (code, mut payload) = Self::parse_event(data)?;
        match code {
            EventCode::CommandComplete => {
                // HCI event packet ([Vol 4] Part E, Section 7.7.14).
                let (_cmd_quota, opcode) = Option::zip(payload.get_u8(), payload.get_u16().map(Opcode::from))
                    .ok_or(Error::BadEventPacketSize)?;
                trace!("Received CommandComplete for {:?}", opcode);
                let (_, tx) = {
                    let mut commands = self.commands.lock();
                    let pos = commands.iter().position(|(op, _)| *op == opcode)
                        .ok_or(Error::UnexpectedCommandResponse(opcode))?;
                    commands.remove(pos).unwrap()
                };
                tx.send(payload).unwrap_or_else(|_| debug!("CommandComplete receiver dropped"));
            }
            _ => debug!("HCI event: {:?} {:?}", code, payload),
        }
        Ok(())
    }

    /// HCI event packet ([Vol 4] Part E, Section 5.4.4).
    fn parse_event(data: &[u8]) -> Result<(EventCode, ReceiveBuffer), Error> {
        data
            .split_first_chunk()
            .ok_or(Error::BadEventPacketSize)
            .and_then(|([code, len], payload)| {
                let code = EventCode::try_from(*code)
                    .map_err(|_| Error::UnknownEventCode(*code))?;
                ensure!(*len as usize == payload.len(), Error::BadEventPacketSize);
                Ok((code, ReceiveBuffer::from_payload(payload)))
            })
    }
}

pub trait FromEvent: Sized {
    fn unpack(buf: &mut ReceiveBuffer) -> Option<Self>;
}

impl FromEvent for () {
    fn unpack(_: &mut ReceiveBuffer) -> Option<Self> {
        Some(())
    }
}