use std::collections::BTreeMap;
use std::fmt::Debug;
use std::future::Future;
use std::time::Duration;

use bytes::{Bytes, BytesMut};
use instructor::{BigEndian, Buffer, BufferMut, Exstruct};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::oneshot::Sender as OneshotSender;

use crate::avc::{CommandCode, PassThroughFrame, PassThroughOp, PassThroughState};
use crate::avrcp::error::Error;
use crate::avrcp::packets::{EventId, MediaAttributeId, Pdu, EVENTS_SUPPORTED_CAPABILITY};
use crate::ensure;
use crate::utils::FromStruct;

pub type CommandResponseSender = OneshotSender<Result<Bytes, Error>>;
#[derive(Debug)]
pub enum AvrcpCommand {
    PassThrough(PassThroughOp, PassThroughState, CommandResponseSender),
    VendorSpecific(CommandCode, Pdu, Bytes, CommandResponseSender),
    RegisterNotification(EventId, u32, EventParser, CommandResponseSender),
    UpdatedVolume(f32)
}

impl AvrcpCommand {
    pub fn into_response_sender(self) -> Option<CommandResponseSender> {
        match self {
            AvrcpCommand::PassThrough(_, _, tx) => Some(tx),
            AvrcpCommand::VendorSpecific(_, _, _, tx) => Some(tx),
            AvrcpCommand::RegisterNotification(_, _, _, tx) => Some(tx),
            _ => None
        }
    }
}

pub struct AvrcpSession {
    pub(super) commands: Sender<AvrcpCommand>,
    pub(super) events: Receiver<Event>
}

impl Debug for AvrcpSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AvrcpSession").finish()
    }
}

impl AvrcpSession {
    pub fn next_event(&mut self) -> impl Future<Output = Option<Event>> + '_ {
        self.events.recv()
    }

    async fn send_vendor_cmd(&self, code: CommandCode, pdu: Pdu, parameters: Bytes) -> Result<Bytes, Error> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.commands
            .send(AvrcpCommand::VendorSpecific(code, pdu, parameters, tx))
            .await
            .map_err(|_| Error::SessionClosed)?;
        rx.await.map_err(|_| Error::SessionClosed)?
    }

    async fn send_action(&self, op: PassThroughOp, state: PassThroughState) -> Result<(), Error> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.commands
            .send(AvrcpCommand::PassThrough(op, state, tx))
            .await
            .map_err(|_| Error::SessionClosed)?;
        let mut result = rx.await.map_err(|_| Error::SessionClosed)??;
        let frame: PassThroughFrame = result.read_be()?;
        ensure!(frame.op == op && frame.state == state, Error::InvalidReturnData);
        Ok(())
    }

    pub async fn notify_local_volume_change(&self, volume: f32) -> Result<(), Error> {
        self.commands
            .send(AvrcpCommand::UpdatedVolume(volume))
            .await
            .map_err(|_| Error::SessionClosed)
    }

    pub async fn action(&self, op: PassThroughOp) -> Result<(), Error> {
        self.send_action(op, PassThroughState::Pressed)
            .await?;
        self.send_action(op, PassThroughState::Released)
            .await?;
        Ok(())
    }

    pub async fn get_supported_events(&self) -> Result<Vec<EventId>, Error> {
        let mut result = self
            .send_vendor_cmd(
                CommandCode::Status,
                Pdu::GetCapabilities,
                Bytes::from_struct_be(EVENTS_SUPPORTED_CAPABILITY)
            )
            .await?;
        ensure!(result.read_be::<u8>()? == EVENTS_SUPPORTED_CAPABILITY, Error::InvalidReturnData);
        let number_of_events: u8 = result.read_be()?;
        let mut events = Vec::with_capacity(number_of_events as usize);
        for _ in 0..number_of_events {
            events.push(result.read_be()?);
        }
        Ok(events)
    }

    pub async fn register_notification<N: Notification>(&self, playback_interval: Option<Duration>) -> Result<N, Error> {
        assert!(N::EVENT_ID != EventId::PlaybackPosChanged || playback_interval.is_some(), "PlaybackPosChanged requires an interval");
        let (tx, rx) = tokio::sync::oneshot::channel();
        let int = playback_interval.map_or(0, |interval| interval.as_secs() as u32);
        self.commands
            .send(AvrcpCommand::RegisterNotification(N::EVENT_ID, int, N::read, tx))
            .await
            .map_err(|_| Error::SessionClosed)?;
        let mut result = rx.await.map_err(|_| Error::SessionClosed)??;
        ensure!(result.read_be::<EventId>()? == N::EVENT_ID, Error::InvalidReturnData);
        let notification: N = result.read_be()?;
        result.finish()?;
        Ok(notification)
    }

    // ([AVRCP] Section 6.6.1)
    pub async fn get_current_media_attributes(
        &self, filter: Option<&[MediaAttributeId]>
    ) -> Result<BTreeMap<MediaAttributeId, String>, Error> {
        const PLAYING: u64 = 0x00;
        const UTF8: u16 = 106;
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
        let mut result = self
            .send_vendor_cmd(CommandCode::Status, Pdu::GetElementAttributes, buffer.freeze())
            .await?;
        let number_of_attributes: u8 = result.read_be()?;
        let mut results = BTreeMap::new();
        for _ in 0..number_of_attributes {
            let id: MediaAttributeId = result.read_be()?;
            ensure!(result.read_be::<u16>()? == UTF8, Error::InvalidReturnData);
            let length: u16 = result.read_be()?;
            let value = result.split_to(length as usize);
            results.insert(id, String::from_utf8_lossy(&value).to_string());
        }
        Ok(results)
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

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    TrackChanged(notifications::CurrentTrack),
    PlaybackStatusChanged(notifications::PlaybackStatus),
    PlaybackPositionChanged(notifications::PlaybackPosition),
    VolumeChanged(f32)
}

pub mod notifications {
    use std::time::Duration;
    use instructor::{BigEndian, Buffer, Error, Exstruct};

    use crate::avrcp::packets::EventId;
    use crate::avrcp::session::Notification;
    use crate::avrcp::Event;

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
                u64::MAX => Self::NotSelected,
                u64::MIN => Self::Selected,
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

    #[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Exstruct)]
    #[repr(u8)]
    pub enum PlaybackStatus {
        #[default]
        Stopped = 0x00,
        Playing = 0x01,
        Paused = 0x02,
        FwdSeek = 0x03,
        RevSeek = 0x04,
        #[instructor(default)]
        Error = 0xFF
    }

    impl From<PlaybackStatus> for Event {
        fn from(value: PlaybackStatus) -> Self {
            Event::PlaybackStatusChanged(value)
        }
    }

    impl Notification for PlaybackStatus {
        const EVENT_ID: EventId = EventId::PlaybackStatusChanged;
    }

    #[derive(Default, Debug, Copy, Clone, Eq, PartialEq)]
    pub enum PlaybackPosition {
        #[default]
        NotSelected,
        Position(Duration)
    }

    impl PlaybackPosition {
        pub fn as_option(&self) -> Option<Duration> {
            match self {
                Self::Position(duration) => Some(*duration),
                _ => None
            }
        }
    }

    impl Exstruct<BigEndian> for PlaybackPosition {
        fn read_from_buffer<B: Buffer>(buffer: &mut B) -> Result<Self, Error> {
            let pos: u32 = buffer.read_be()?;
            Ok(match pos {
                u32::MAX => Self::NotSelected,
                i => Self::Position(Duration::from_millis(i as u64))
            })
        }
    }

    impl From<PlaybackPosition> for Event {
        fn from(event: PlaybackPosition) -> Self {
            Self::PlaybackPositionChanged(event)
        }
    }

    impl Notification for PlaybackPosition {
        const EVENT_ID: EventId = EventId::PlaybackPosChanged;
    }

}
