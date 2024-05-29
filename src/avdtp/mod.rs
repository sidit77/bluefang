pub mod packets;
pub mod error;
pub mod endpoint;
pub mod utils;
pub mod capabilities;

use std::collections::BTreeMap;
use std::sync::Arc;
use bytes::{Bytes, BytesMut};
use instructor::{Buffer, BufferMut};
use parking_lot::Mutex;
use tokio::{select, spawn};
use tokio::runtime::Handle;
use tokio::sync::oneshot::{Receiver, Sender};
use tracing::{debug, info, trace, warn};
use crate::avdtp::endpoint::Stream;
use crate::avdtp::error::ErrorCode;
use crate::avdtp::packets::{MessageType, SignalChannelExt, SignalIdentifier, SignalMessage, SignalMessageAssembler};
use crate::hci::Error;
use crate::l2cap::channel::Channel;
use crate::l2cap::Server;
use crate::utils::{MutexCell, select_all, stall_if_none};

pub use endpoint::{StreamHandler, LocalEndpoint};
use crate::avdtp::capabilities::Capability;

#[derive(Default)]
pub struct AvdtpServerBuilder {
    endpoints: Vec<LocalEndpoint>,
}

impl AvdtpServerBuilder {

    pub fn with_endpoint(mut self, endpoint: LocalEndpoint) -> Self {
        self.endpoints.push(endpoint);
        self
    }

    pub fn build(self) -> AvdtpServer {
        AvdtpServer {
            pending_streams: Arc::new(Mutex::new(BTreeMap::new())),
            local_endpoints: self.endpoints.into(),
        }
    }
}

type ChannelSender = MutexCell<Option<Sender<Channel>>>;
pub struct AvdtpServer {
    pending_streams: Arc<Mutex<BTreeMap<u16, Arc<ChannelSender>>>>,
    local_endpoints: Arc<[LocalEndpoint]>,
}

impl Server for AvdtpServer {
    fn on_connection(&mut self, mut channel: Channel) {
        let handle = channel.connection_handle;
        let pending_stream = self.pending_streams.lock().get(&handle).cloned();
        match pending_stream {
            None => {
                trace!("New AVDTP session (signaling channel)");
                let pending_streams = self.pending_streams.clone();
                let pending_stream = Arc::new(ChannelSender::default());
                pending_streams.lock().insert(handle, pending_stream.clone());

                let local_endpoints = self.local_endpoints.clone();

                // Use an OS thread instead a tokio task to avoid blocking the runtime with audio processing
                let runtime = Handle::current();
                std::thread::spawn(move || runtime.block_on(async move {
                    if let Err(err) = channel.configure().await {
                        warn!("Error configuring channel: {:?}", err);
                        return;
                    }
                    let mut session = AvdtpSession {
                        channel_sender: pending_stream,
                        channel_receiver: None,
                        local_endpoints,
                        streams: Vec::new(),
                    };
                    session.handle_control_channel(channel).await.unwrap_or_else(|err| {
                        warn!("Error handling control channel: {:?}", err);
                    });
                    trace!("AVDTP signaling session ended for 0x{:04x}", handle);
                    pending_streams.lock().remove(&handle);
                }));
            }
            Some(pending) => {
                trace!("Existing AVDTP session (transport channel)");
                spawn(async move {
                    if let Err(err) = channel.configure().await {
                        warn!("Error configuring channel: {:?}", err);
                        return;
                    }
                    pending
                        .take()
                        .expect("Unexpected AVDTP transport connection")
                        .send(channel)
                        .unwrap_or_else(|_| panic!("Failed to send channel to session"));
                });
            }
        }
    }
}

struct AvdtpSession {
    channel_sender: Arc<ChannelSender>,
    channel_receiver: Option<Receiver<Channel>>,
    local_endpoints: Arc<[LocalEndpoint]>,
    streams: Vec<Stream>,
}

impl AvdtpSession {

    async fn handle_control_channel(&mut self, mut channel: Channel) -> Result<(), Error> {
        let mut assembler = SignalMessageAssembler::default();
        loop {
            select! {
                (i, _) = select_all(&mut self.streams) => {
                    debug!("Stream {} ended", i);
                    self.streams.swap_remove(i);
                },
                signal = channel.read() => match signal {
                    Some(packet) => match assembler.process_msg(packet) {
                        Ok(Some(header)) => {
                            let reply = self.handle_signal_message(header);
                            channel.send_signal(reply)?;
                        }
                        Ok(None) => continue,
                        Err(err) => {
                            warn!("Error processing signaling message: {:?}", err);
                            continue;
                        }
                    },
                    None => break,
                },
                res = stall_if_none(&mut self.channel_receiver) => {
                    let channel = res.expect("Channel receiver closed");
                    self.streams
                        .iter_mut()
                        .find(|stream| stream.is_opening())
                        .map(|stream| stream.set_channel(channel))
                        .unwrap_or_else(|| warn!("No stream waiting for channel"));
                    self.channel_receiver = None;
                }
            }
        }
        Ok(())
    }

    fn handle_signal_message(&mut self, msg: SignalMessage) -> SignalMessage {
        assert_eq!(msg.message_type, MessageType::Command);
        let resp = SignalMessageResponse::for_msg(&msg);
        let mut data = msg.data;
        match msg.signal_identifier {
            // ([AVDTP] Section 8.6).
            SignalIdentifier::Discover => resp.try_accept(|buf| {
                data.finish()?;
                for endpoint in self.local_endpoints.iter() {
                    buf.write(&endpoint.as_stream_endpoint());
                }
                Ok(())
            }),
            // ([AVDTP] Section 8.7).
            SignalIdentifier::GetCapabilities => resp.general_reject(),
            // ([AVDTP] Section 8.8).
            SignalIdentifier::GetAllCapabilities => resp.try_accept(|buf| {
                let seid = data.read_be::<u8>()? >> 2;
                data.finish()?;
                let ep = self.local_endpoints.iter()
                    .find(|ep| ep.seid == seid)
                    .ok_or(ErrorCode::BadAcpSeid)?;
                buf.write(&ep.capabilities);
                Ok(())
            }),
            // ([AVDTP] Section 8.9).
            SignalIdentifier::SetConfiguration => resp.try_accept(|_| {
                //TODO add the required parameters to a reject
                let acp_seid = data.read_be::<u8>()? >> 2;
                let int_seid = data.read_be::<u8>()? >> 2;
                let capabilities: Vec<Capability> = data.read_be()?;
                data.finish()?;
                let ep = self.local_endpoints.iter()
                    .find(|ep| ep.seid == acp_seid)
                    .ok_or(ErrorCode::BadAcpSeid)?;
                self.streams.push(Stream::new(ep, int_seid, capabilities)?);
                Ok(())
            }),
            // ([AVDTP] Section 8.10).
            SignalIdentifier::GetConfiguration => resp.general_reject(),
            // ([AVDTP] Section 8.11).
            SignalIdentifier::Reconfigure => resp.general_reject(),
            // ([AVDTP] Section 8.12).
            SignalIdentifier::Open => resp.try_accept(|_| {
                let seid = data.read_be::<u8>()? >> 2;
                data.finish()?;
                let stream = self.streams.iter_mut()
                    .find(|stream| stream.local_endpoint == seid)
                    .ok_or(ErrorCode::BadAcpSeid)?;
                //info!("OPEN (0x{:02x}): {:?}", seid, sink.media_type);
                stream.set_to_opening()?;
                let (tx, rx) = tokio::sync::oneshot::channel();
                self.channel_sender.set(Some(tx));
                self.channel_receiver = Some(rx);
                Ok(())
            }),
            // ([AVDTP] Section 8.13).
            SignalIdentifier::Start => resp.try_accept(|_| {
                //TODO handle rejects correctly
                while {
                    let seid = data.read_be::<u8>()? >> 2;
                    data.finish()?;
                    let sink = self.streams.iter_mut()
                        .find(|stream| stream.local_endpoint == seid)
                        .ok_or(ErrorCode::BadAcpSeid)?;
                    info!("START {:?}", seid);
                    sink.start()?;
                    !data.is_empty()
                } {}
                Ok(())
            }),
            // ([AVDTP] Section 8.14).
            SignalIdentifier::Close => resp.try_accept(|_| {
                let seid = data.read_be::<u8>()? >> 2;
                data.finish()?;
                let stream = self.streams.iter_mut()
                    .find(|stream| stream.local_endpoint == seid)
                    .ok_or(ErrorCode::BadAcpSeid)?;
                stream.close()?;
                info!("SUSPEND {:?}", seid);
                Ok(())
            }),
            // ([AVDTP] Section 8.15).
            SignalIdentifier::Suspend => resp.try_accept(|_| {
                //TODO handle rejects correctly
                while {
                    let seid = data.read_be::<u8>()? >> 2;
                    data.finish()?;
                    let sink = self.streams.iter_mut()
                        .find(|stream| stream.local_endpoint == seid)
                        .ok_or(ErrorCode::BadAcpSeid)?;
                    info!("SUSPEND {:?}", seid);
                    sink.stop()?;
                    !data.is_empty()
                } {}
                Ok(())
            }),
            // ([AVDTP] Section 8.16).
            SignalIdentifier::Abort => resp.general_reject(),
            // ([AVDTP] Section 8.17).
            SignalIdentifier::SecurityControl => resp.general_reject(),
            // ([AVDTP] Section 8.18).
            SignalIdentifier::Unknown => resp.general_reject(),
            // ([AVDTP] Section 8.19).
            SignalIdentifier::DelayReport => resp.general_reject()
        }
    }
}


struct SignalMessageResponse {
    transaction_label: u8,
    signal_identifier: SignalIdentifier,
}

impl SignalMessageResponse {

    pub fn for_msg(msg: &SignalMessage) -> Self {
        Self {
            transaction_label: msg.transaction_label,
            signal_identifier: msg.signal_identifier,
        }
    }

    pub fn general_reject(&self) -> SignalMessage {
        warn!("Unsupported signaling message: {:?}", self.signal_identifier);
        SignalMessage {
            transaction_label: self.transaction_label,
            message_type: MessageType::GeneralReject,
            signal_identifier: self.signal_identifier,
            data: Bytes::new(),
        }
    }

    pub fn try_accept<F: FnOnce(&mut BytesMut) -> Result<(), ErrorCode>>(&self, f: F) -> SignalMessage {
        let mut buf = BytesMut::new();
        match f(&mut buf) {
            Ok(()) => SignalMessage {
                transaction_label: self.transaction_label,
                message_type: MessageType::ResponseAccept,
                signal_identifier: self.signal_identifier,
                data: buf.freeze(),
            },
            Err(reason) => self.reject(reason),
        }
    }

    pub fn reject(&self, reason: ErrorCode) -> SignalMessage {
        warn!("Rejecting signal {:?} because of {:?}", self.signal_identifier, reason);
        SignalMessage {
            transaction_label: self.transaction_label,
            message_type: MessageType::ResponseReject,
            signal_identifier: self.signal_identifier,
            data: {
                let mut buf = BytesMut::new();
                buf.write_be(&reason);
                buf.freeze()
            },
        }
    }

    //pub fn accept<F: FnOnce(&mut BytesMut)>(&self, f: F) -> SignalMessage {
    //    SignalMessage {
    //        transaction_label: self.transaction_label,
    //        message_type: MessageType::ResponseAccept,
    //        signal_identifier: self.signal_identifier,
    //        data: {
    //            let mut buf = BytesMut::new();
    //            f(&mut buf);
    //            buf.freeze()
    //        },
    //    }
    //}

}
