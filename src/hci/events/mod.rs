use std::collections::VecDeque;
use std::mem::size_of;
use parking_lot::Mutex;
use smallvec::SmallVec;
use tokio::sync::oneshot::{Receiver, Sender};
use tracing::{debug, trace};
use crate::ensure;
use crate::hci::buffer::ReceiveBuffer;
use crate::hci::commands::Opcode;
use crate::hci::consts::{ClassOfDevice, EventCode, LinkType, RemoteAddr, Status};
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
            EventCode::CommandComplete | EventCode::CommandStatus => {
                // ([Vol 4] Part E, Section 7.7.14).
                // ([Vol 4] Part E, Section 7.7.15).
                if code == EventCode::CommandStatus {
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
            EventCode::InquiryComplete => {
                // ([Vol 4] Part E, Section 7.7.1).
                let status = Status::from(payload.u8()?);
                payload.finish()?;
                debug!("Inquiry complete: {}", status);
            },
            EventCode::InquiryResult => {
                // ([Vol 4] Part E, Section 7.7.2).
                let count = payload.u8()? as usize;
                let addr: SmallVec<[RemoteAddr; 2]> = (0..count)
                    .map(|_| payload.bytes().map(RemoteAddr::from))
                    .collect::<Result<_, _>>()?;
                payload.skip(count * 3); // repetition mode
                let classes: SmallVec<[ClassOfDevice; 2]> = (0..count)
                    .map(|_| payload
                        .u24()
                        .map(ClassOfDevice::from))
                    .collect::<Result<_, _>>()?;
                payload.skip(count * 2); // clock offset
                payload.finish()?;

                for i in 0..count {
                    debug!("Inquiry result: {} {:?}", addr[i], classes[i]);
                }
            },
            EventCode::ConnectionComplete => {
                // ([Vol 4] Part E, Section 7.7.3).
                let status = payload.u8().map(Status::from)?;
                let handle = payload.u16()?;
                let addr = payload.bytes().map(RemoteAddr::from)?;
                let link_type = payload.u8().map(LinkType::from)?;
                let encryption_enabled = payload.u8().map(|b| b == 0x01)?;
                payload.finish()?;
                debug!("Connection complete: {} {} {:?} {} -> {}",
                    status, addr, link_type, encryption_enabled, handle);
            },
            EventCode::ConnectionRequest => {
                // ([Vol 4] Part E, Section 7.7.4).
                let addr = payload.bytes().map(RemoteAddr::from)?;
                let class = payload.u24().map(ClassOfDevice::from)?;
                let link_type = payload.u8().map(LinkType::from)?;
                payload.finish()?;
                debug!("Connection request: {} {:?} {:?}", addr, class, link_type);
            },
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