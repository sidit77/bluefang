use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::spawn;
use tracing::{trace, warn};
use crate::l2cap::channel::Channel;
use crate::l2cap::Server;

#[derive(Default)]
pub struct AvdtpServer {
    sessions: BTreeMap<u16, Arc<AvdtpSession>>
}

impl Server for AvdtpServer {
    fn on_connection(&mut self, mut channel: Channel) {
        let handle = channel.connection_handle;
        match self.sessions.get(&handle) {
            None => {
                trace!("New AVDTP session (signaling channel)");
                let session = Arc::new(AvdtpSession {});
                self.sessions.insert(handle, session.clone());
                spawn(async move {
                    if let Err(err) = channel.configure().await {
                        warn!("Error configuring channel: {:?}", err);
                        return;
                    }
                    session.handle_control_channel(channel).await;
                    trace!("AVDTP signaling session ended for 0x{:04x}", handle);
                });
            }
            Some(session) => {
                trace!("Existing AVDTP session (transport channel)");
                let session = session.clone();
                spawn(async move {
                    if let Err(err) = channel.configure().await {
                        warn!("Error configuring channel: {:?}", err);
                        return;
                    }
                    session.handle_transport_channel(channel).await;
                    trace!("AVDTP transport session ended for 0x{:04x}", handle);
                });
            }
        }
    }
}

struct AvdtpSession {

}

impl AvdtpSession {

    async fn handle_control_channel(&self, _channel: Channel) {
        trace!("Handling control channel");
    }

    async fn handle_transport_channel(&self, _channel: Channel) {
        trace!("Handling signaling channel");
    }
}