use smallvec::SmallVec;
use tracing::debug;
use crate::ensure;
use crate::hci::buffer::ReceiveBuffer;
use crate::hci::consts::{EventCode, Opcode, Status};
use crate::hci::Error;

#[derive(Default)]
pub struct EventRouter {

}

impl EventRouter {
    pub fn handle_event(&self, data: &[u8]) -> Result<(), Error> {
        let (code, mut payload) = Self::parse_event(data)?;
        match code {
            EventCode::CommandComplete => {
                let rem_packets = payload.get_u8().unwrap();
                let opcode = Opcode::from(payload.get_u16().unwrap());
                let status = Status::from(payload.get_u8().unwrap());
                debug!("HCI event: CommandComplete {:?} {:?} {:?}", rem_packets, opcode, status);
            }
            _ => debug!("HCI event: {:?} {:?}", code, payload),
        }
        Ok(())
    }

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

pub trait FromEvent {
    fn unpack(buf: &mut ReceiveBuffer) -> Self;
}

impl FromEvent for () {
    fn unpack(_: &mut ReceiveBuffer) -> Self {
        ()
    }
}