use std::future::Future;
use bytes::{Bytes};
use instructor::{BigEndian, Buffer, Exstruct};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::oneshot::Sender as OneshotSender;
use crate::avc::{CommandCode, PassThroughFrame, PassThroughOp, PassThroughState};
use crate::avrcp::notifications::TrackChanged;
use crate::avrcp::packets::{EventId, EVENTS_SUPPORTED_CAPABILITY, Pdu};
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

}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SessionError {
    SessionClosed,
    NoTransactionIdAvailable,
    NotImplemented,
    Rejected,
    Busy,
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
    TrackChanged(TrackChanged),
}


pub mod notifications {
    use instructor::Exstruct;
    use crate::avrcp::Event;
    use crate::avrcp::packets::EventId;
    use crate::avrcp::session::Notification;

    #[derive(Debug, Copy, Clone, PartialEq, Eq, Exstruct)]
    pub struct TrackChanged {
        pub identifier: u64
    }

    impl From<TrackChanged> for Event {
        fn from(event: TrackChanged) -> Self {
            Self::TrackChanged(event)
        }
    }

    impl Notification for TrackChanged {
        const EVENT_ID: EventId = EventId::TrackChanged;
    }
}