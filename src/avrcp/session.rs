use bytes::{Bytes};
use instructor::{Buffer};
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot::Sender as OneshotSender;
use crate::avc::{CommandCode, PassThroughFrame, PassThroughOp, PassThroughState};
use crate::avrcp::packets::{Event, EVENTS_SUPPORTED_CAPABILITY, Pdu};
use crate::ensure;
use crate::utils::to_bytes_be;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AvrcpCommand {
    PassThrough(PassThroughOp, PassThroughState),
    VendorSpecific(CommandCode, Pdu, Bytes),
}

pub struct AvrcpSession {
    pub(super) player_commands: Sender<(AvrcpCommand, OneshotSender<Result<Bytes, SessionError>>)>
}

impl AvrcpSession {
    async fn send_cmd(&self, cmd: AvrcpCommand) -> Result<Bytes, SessionError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.player_commands.send((cmd, tx)).await.map_err(|_| SessionError::SessionClosed)?;
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

    pub async fn get_supported_events(&self) -> Result<Vec<Event>, SessionError> {
        let mut result = self.send_cmd(AvrcpCommand::VendorSpecific(CommandCode::Status, Pdu::GetCapabilities, to_bytes_be(EVENTS_SUPPORTED_CAPABILITY))).await?;
        ensure!(result.read_be::<u8>()? == EVENTS_SUPPORTED_CAPABILITY, SessionError::InvalidReturnData);
        let number_of_events: u8 = result.read_be()?;
        let mut events = Vec::with_capacity(number_of_events as usize);
        for _ in 0..number_of_events {
            events.push(result.read_be()?);
        }
        Ok(events)
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
