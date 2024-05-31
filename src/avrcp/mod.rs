use std::collections::BTreeSet;
use std::sync::Arc;
use parking_lot::Mutex;
use tokio::spawn;
use tracing::{trace, warn};
use crate::avctp::Avctp;
use crate::avrcp::sdp::REMOTE_CONTROL_SERVICE;
use crate::hci;
use crate::l2cap::channel::Channel;
use crate::l2cap::{AVCTP_PSM, ProtocolDelegate, ProtocolHandler, ProtocolHandlerProvider};

pub mod sdp;

#[derive(Default)]
pub struct AvrcpBuilder;

impl AvrcpBuilder {
    pub fn build(self) -> Avrcp {
        Avrcp {
            existing_connections: Arc::new(Mutex::new(BTreeSet::new()))
        }
    }
}

#[derive(Clone)]
pub struct Avrcp {
    existing_connections: Arc<Mutex<BTreeSet<u16>>>
}

impl ProtocolHandlerProvider for Avrcp {
    fn protocol_handlers(&self) -> Vec<Box<dyn ProtocolHandler>> {
        vec![
            ProtocolDelegate::new(AVCTP_PSM, self.clone(), Self::handle_control)
        ]
    }
}

impl Avrcp {
    pub fn handle_control(&self, mut channel: Channel) {
        let handle = channel.connection_handle;
        let success = self.existing_connections.lock().insert(handle);
        if success {
            let existing_connections = self.existing_connections.clone();
            spawn(async move {
                if let Err(err) = channel.configure().await {
                    warn!("Error configuring channel: {:?}", err);
                    return;
                }
                let mut state = State { };
                state.run(channel).await.unwrap_or_else(|err| {
                    warn!("Error running avctp: {:?}", err);
                });
                trace!("AVCTP connection closed");
                existing_connections.lock().remove(&handle);
            });
        }
    }
}

struct State {

}

impl State {
    async fn run(&mut self, channel: Channel) -> Result<(), hci::Error> {
        let mut avctp = Avctp::new(channel, [REMOTE_CONTROL_SERVICE]);
        while let Some(packet) = avctp.read().await {
            println!("Received packet: {:?}", packet);
        }
        Ok(())
    }
}