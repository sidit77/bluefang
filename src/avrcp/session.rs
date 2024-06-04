use tokio::sync::mpsc::Sender;
use crate::avc::{PassThroughOp, PassThroughState};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum AvrcpCommand {
    PassThrough(PassThroughOp, PassThroughState),
}

pub struct AvrcpSession {
    pub(super) player_commands: Sender<AvrcpCommand>
}

impl AvrcpSession {
    async fn send_action(&self, op: PassThroughOp) {
        self.player_commands.send(AvrcpCommand::PassThrough(op, PassThroughState::Pressed)).await.unwrap();
        self.player_commands.send(AvrcpCommand::PassThrough(op, PassThroughState::Released)).await.unwrap();
    }

    pub async fn play(&self) {
        self.send_action(PassThroughOp::Play).await;
    }

    pub async fn pause(&self) {
        self.send_action(PassThroughOp::Pause).await;
    }

}