mod packets;

use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::spawn;
use tracing::{trace, warn};
use crate::avdtp::packets::{SignalIdentifier, SignalMessageAssembler};
use crate::hci::Error;
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
                    session.handle_control_channel(channel).await.unwrap_or_else(|err| {
                        warn!("Error handling control channel: {:?}", err);
                    });
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

    async fn handle_control_channel(&self, mut channel: Channel) -> Result<(), Error> {
        let mut assembler = SignalMessageAssembler::default();
        while let Some(packet) = channel.read().await {
            match assembler.process_msg(packet) {
                Ok(Some(header)) => {
                    trace!("Received signaling message: {:?}", header);
                    match header.signal_identifier {
                        SignalIdentifier::Discover => {

                        }
                        _ => warn!("Unsupported signaling message: {:?}", header.signal_identifier)
                    }
                }
                Ok(None) => continue,
                Err(err) => {
                    warn!("Error processing signaling message: {:?}", err);
                    continue;
                }
            }
        }
        Ok(())
    }

    async fn handle_transport_channel(&self, _channel: Channel) {
        trace!("Handling signaling channel");
    }
}