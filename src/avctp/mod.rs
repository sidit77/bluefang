mod packets;

use std::collections::BTreeSet;
use std::sync::Arc;
use parking_lot::Mutex;
use tokio::spawn;
use tracing::{info, trace, warn};
use crate::avctp::packets::MessageAssembler;
use crate::hci;
use crate::l2cap::channel::Channel;
use crate::l2cap::{AVCTP_PSM, ProtocolHandler};

#[derive(Default)]
pub struct AvctpBuilder;

impl AvctpBuilder {
    pub fn build(self) -> Avctp {
        Avctp {
            existing_connections: Arc::new(Default::default()),
        }
    }
}

#[derive(Clone)]
pub struct Avctp {
    existing_connections: Arc<Mutex<BTreeSet<u16>>>
}

impl ProtocolHandler for Avctp {
    fn psm(&self) -> u64 {
        AVCTP_PSM as u64
    }

    fn handle(&self, mut channel: Channel) {
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
    async fn run(&mut self, mut channel: Channel) -> Result<(), hci::Error> {
        let mut assembler = MessageAssembler::default();
        while let Some(packet) = channel.read().await {
            match assembler.process_msg(packet) {
                Ok(Some(msg)) => {
                    info!("Received message: {:?}", msg);
                }
                Ok(None) => continue,
                Err(err) => {
                    warn!("Error processing message: {:?}", err);
                    continue;
                }
            }
        }
        Ok(())
    }
}