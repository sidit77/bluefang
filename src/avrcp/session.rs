use std::future::Future;
use bytes::{Bytes, BytesMut};
use instructor::{BigEndian, Buffer, BufferMut, Exstruct};
use thiserror::Error;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::oneshot::Sender as OneshotSender;
use crate::avc::{CommandCode, PassThroughFrame, PassThroughOp, PassThroughState};
use crate::avrcp::notifications::CurrentTrack;
use crate::avrcp::packets::{EventId, EVENTS_SUPPORTED_CAPABILITY, MediaAttributeId, Pdu};
use crate::ensure;
use crate::utils::to_bytes_be;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AvrcpCommand {
    PassThrough(PassThroughOp, PassThroughState),
    VendorSpecific(CommandCode, Pdu, Bytes),
    RegisterNotification(EventId, EventParser)
}

pub struct AvrcpSession {
    pub(super) commands: Sender<(AvrcpCommand, OneshotSender<Result<Bytes, SessionError>>)>,
    pub(super) events: Receiver<Event>
}

impl AvrcpSession {

    pub fn next_event(&mut self) -> impl Future<Output = Option<Event>> + '_ {
        self.events.recv()
    }

    async fn send_cmd(&self, cmd: AvrcpCommand) -> Result<Bytes, SessionError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.commands.send((cmd, tx)).await.map_err(|_| SessionError::SessionClosed)?;
        rx.await.map_err(|_| SessionError::SessionClosed)?
    }

    async fn send_action(&self, op: PassThroughOp, state: PassThroughState) -> Result<(), SessionError> {
        let mut result = self.send_cmd(AvrcpCommand::PassThrough(op, state)).await?;
        let frame: PassThroughFrame = result.read_be()?;
        ensure!(frame.op == op && frame.state == state, SessionError::InvalidReturnData);
        Ok(())
    }

    pub async fn play(&self) -> Result<(), SessionError> {
        self.send_action(PassThroughOp::Play, PassThroughState::Pressed).await?;
        self.send_action(PassThroughOp::Play, PassThroughState::Released).await?;
        Ok(())
    }

    pub async fn pause(&self) -> Result<(), SessionError> {
        self.send_action(PassThroughOp::Pause, PassThroughState::Pressed).await?;
        self.send_action(PassThroughOp::Pause, PassThroughState::Released).await?;
        Ok(())
    }

    pub async fn get_supported_events(&self) -> Result<Vec<EventId>, SessionError> {
        let mut result = self.send_cmd(AvrcpCommand::VendorSpecific(CommandCode::Status, Pdu::GetCapabilities, to_bytes_be(EVENTS_SUPPORTED_CAPABILITY))).await?;
        ensure!(result.read_be::<u8>()? == EVENTS_SUPPORTED_CAPABILITY, SessionError::InvalidReturnData);
        let number_of_events: u8 = result.read_be()?;
        let mut events = Vec::with_capacity(number_of_events as usize);
        for _ in 0..number_of_events {
            events.push(result.read_be()?);
        }
        Ok(events)
    }

    pub async fn register_notification<N: Notification>(&self) -> Result<N, SessionError> {
        let mut result = self.send_cmd(AvrcpCommand::RegisterNotification(N::EVENT_ID, N::read)).await?;
        ensure!(result.read_be::<EventId>()? == N::EVENT_ID, SessionError::InvalidReturnData);
        let notification: N = result.read_be()?;
        result.finish()?;
        Ok(notification)
    }

    pub async fn get_current_media_attributes(&self, filter: Option<&[MediaAttributeId]>) -> Result<(), SessionError> {
        const PLAYING: u8 = 0x00;
        debug_assert!(filter.map_or(true, |filter| !filter.is_empty()), "Filter should not be empty");
        let mut buffer = BytesMut::new();
        buffer.write_be(PLAYING);
        match filter {
            None => buffer.write_be(0u8),
            Some(filter) => {
                buffer.write_be(filter.len() as u8);
                for &id in filter {
                    buffer.write_be(id);
                }
            }
        }
        Ok(())
    }

}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Error)]
pub enum SessionError {
    #[error("The AVRCP session has been closed.")]
    SessionClosed,
    #[error("All 16 transaction ids are currently occupied.")]
    NoTransactionIdAvailable,
    #[error("The receiver does not implemented the command.")]
    NotImplemented,
    #[error("The receiver rejected the command.")]
    Rejected,
    #[error("The receiver is currently unable to perform this action due to being in a transient state.")]
    Busy,
    #[error("The returned data has an invalid format.")]
    InvalidReturnData
}

impl From<instructor::Error> for SessionError {
    fn from(_: instructor::Error) -> Self {
        Self::InvalidReturnData
    }
}

pub type EventParser = fn(&mut Bytes) -> Result<Event, instructor::Error>;
pub trait Notification: Exstruct<BigEndian> + Into<Event> {
    const EVENT_ID: EventId;

    fn read(buffer: &mut Bytes) -> Result<Event, instructor::Error> {
        let event = Self::read_from_buffer(buffer)?;
        Ok(event.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    TrackChanged(CurrentTrack),
}


pub mod notifications {
    use instructor::{BigEndian, Buffer, Error, Exstruct};
    use crate::avrcp::Event;
    use crate::avrcp::packets::EventId;
    use crate::avrcp::session::Notification;

    #[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
    pub enum CurrentTrack {
        #[default]
        NotSelected,
        Selected,
        Id(u64)
    }

    impl Exstruct<BigEndian> for CurrentTrack {
        fn read_from_buffer<B: Buffer>(buffer: &mut B) -> Result<Self, Error> {
            let id: u64 = buffer.read_be()?;
            Ok(match id {
                u64::MIN => Self::NotSelected,
                u64::MAX => Self::Selected,
                i => Self::Id(i)
            })
        }
    }

    impl From<CurrentTrack> for Event {
        fn from(event: CurrentTrack) -> Self {
            Self::TrackChanged(event)
        }
    }

    impl Notification for CurrentTrack {
        const EVENT_ID: EventId = EventId::TrackChanged;
    }
}