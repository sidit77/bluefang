

use crate::hci::buffer::ReceiveBuffer;
use crate::hci::Error;

/*
#[derive(Default)]
pub struct EventRouter {
    commands: Mutex<VecDeque<(Opcode, oneshot::Sender<ReceiveBuffer>)>>,
    connection_manager: Mutex<Option<mpsc::Sender<ParsedConnectionEvent>>>,
}

impl EventRouter {

    pub async fn reserve(&self, opcode: Opcode) -> oneshot::Receiver<ReceiveBuffer> {
        // TODO implement command quota
        let (tx, rx) = oneshot::channel();
        self.commands.lock().push_back((opcode, tx));
        rx
    }

    pub fn connection_events(&self) -> Option<mpsc::Receiver<ParsedConnectionEvent>> {
        let mut manager = self.connection_manager.lock();
        ensure!(manager.is_none() || manager.as_ref().is_some_and(mpsc::Sender::is_closed));
        let (tx, rx) = mpsc::channel(16);
        *manager = Some(tx);
        Some(rx)
    }

    pub fn handle_event(&self, data: &[u8]) -> Result<(), Error> {
        let (code, mut payload) = Self::parse_event(data)?;
        match EventClass::from(code) {
            EventClass::Command(event) => {
                // ([Vol 4] Part E, Section 7.7.14).
                // ([Vol 4] Part E, Section 7.7.15).
                if let CommandEvent::Status = event {
                    payload.get_mut().rotate_left(size_of::<Status>());
                }
                let _cmd_quota = payload.u8()?;
                let opcode= payload.u16().map(Opcode::from)?;
                trace!("Received CommandComplete for {:?}", opcode);
                let (_, tx) = {
                    let mut commands = self.commands.lock();
                    let pos = commands.iter().position(|(op, _)| *op == opcode)
                        .ok_or(Error::UnexpectedCommandResponse(opcode))?;
                    commands.remove(pos).unwrap()
                };
                tx.send(payload).unwrap_or_else(|_| debug!("CommandComplete receiver dropped"));
            },
            EventClass::Connection(event) => self.handle_connection_events(event, payload)?,
            EventClass::Inquiry(event) => self.handle_inquiry_event(event, payload)?,
            EventClass::Unhandled(code) => debug!("Unhandled hci event: {:?} {:?}", code, payload),
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

 */

pub trait FromEvent: Sized {
    fn unpack(buf: &mut ReceiveBuffer) -> Result<Self, Error>;
}

impl FromEvent for () {
    fn unpack(_: &mut ReceiveBuffer) -> Result<Self, Error> {
        Ok(())
    }
}

impl FromEvent for u8 {
    fn unpack(buf: &mut ReceiveBuffer) -> Result<Self, Error> {
        buf.u8()
    }
}

