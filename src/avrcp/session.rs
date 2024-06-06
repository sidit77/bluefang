use bytes::Bytes;
use instructor::{Buffer};
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot::Sender as OneshotSender;
use crate::avc::{PassThroughFrame, PassThroughOp, PassThroughState};
use crate::ensure;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum AvrcpCommand {
    PassThrough(PassThroughOp, PassThroughState),
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
        ensure!(frame.op == op && frame.state == state, instructor::Error::InvalidValue);
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

}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SessionError {
    SessionClosed,
    NoTransactionIdAvailable,
    NotImplemented,
    Rejected,
    InvalidReturnData(instructor::Error)
}

impl From<instructor::Error> for SessionError {
    fn from(value: instructor::Error) -> Self {
        Self::InvalidReturnData(value)
    }
}