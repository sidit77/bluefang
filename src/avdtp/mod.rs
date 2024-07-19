pub mod capabilities;
mod endpoint;
mod error;
mod packets;
pub mod utils;

use std::collections::BTreeMap;
use std::sync::Arc;

use bytes::{Bytes, BytesMut};
use instructor::{BigEndian, Buffer, BufferMut, Instruct};
use parking_lot::Mutex;
use tokio::runtime::Handle;
use tokio::sync::oneshot::{Receiver, Sender};
use tokio::{select, spawn};
use tracing::{debug, trace, warn, error};

use crate::avdtp::capabilities::Capability;
use crate::avdtp::endpoint::Stream;
use crate::avdtp::packets::{MessageType, ServiceCategory, SignalChannelExt, SignalIdentifier, SignalMessage, SignalMessageAssembler};
use crate::ensure;
use crate::l2cap::channel::{Channel, Error as L2capError};
use crate::l2cap::{ProtocolHandler, AVDTP_PSM, L2capServer};
use crate::utils::{select_all, MutexCell, OptionFuture, LoggableResult, IgnoreableResult};

pub use endpoint::{LocalEndpoint, StreamHandler, StreamHandlerFactory};
pub use packets::{MediaType, StreamEndpointType};
use crate::avdtp::error::Error;

#[derive(Default)]
pub struct AvdtpBuilder {
    endpoints: Vec<LocalEndpoint>
}

impl AvdtpBuilder {
    pub fn with_endpoint(mut self, endpoint: LocalEndpoint) -> Self {
        self.endpoints.push(endpoint);
        self
    }

    pub fn build(self) -> Avdtp {
        Avdtp {
            pending_streams: Arc::new(Mutex::new(BTreeMap::new())),
            local_endpoints: self.endpoints.into()
        }
    }
}

type ChannelSender = MutexCell<Option<Sender<Channel>>>;

#[derive(Clone)]
pub struct Avdtp {
    pending_streams: Arc<Mutex<BTreeMap<u16, Arc<ChannelSender>>>>,
    local_endpoints: Arc<[LocalEndpoint]>
}

impl Avdtp {

    pub fn connect(self: Arc<Self>, l2cap: &mut L2capServer, handle: u16) {
        let mut channel = l2cap.new_channel(handle).expect("Failed to create channel");
        spawn(async move {
            channel.connect(self.psm()).await.ignore();
            self.handle(channel);
        });
    }

}

impl ProtocolHandler for Avdtp {
    fn psm(&self) -> u64 {
        AVDTP_PSM as u64
    }

    fn handle(&self, mut channel: Channel) {
        let handle = channel.connection_handle();
        let pending_stream = self.pending_streams.lock().get(&handle).cloned();
        match pending_stream {
            None => {
                trace!("New AVDTP session (signaling channel)");
                let pending_streams = self.pending_streams.clone();
                let pending_stream = Arc::new(ChannelSender::default());
                pending_streams
                    .lock()
                    .insert(handle, pending_stream.clone());

                let local_endpoints = self.local_endpoints.clone();

                if channel.is_response_pending() && channel.accept_connection().log_err().is_err() {
                    return;
                }
                // Use an OS thread instead a tokio task to avoid blocking the runtime with audio processing
                let runtime = Handle::current();
                std::thread::spawn(move || {
                    runtime.block_on(async move {
                        if let Err(err) = channel.configure().await {
                            warn!("Error configuring channel: {:?}", err);
                            return;
                        }
                        let mut session = AvdtpSession {
                            channel_sender: pending_stream,
                            channel_receiver: OptionFuture::never(),
                            local_endpoints,
                            streams: Vec::new()
                        };
                        session
                            .handle_control_channel(channel)
                            .await
                            .unwrap_or_else(|err| {
                                warn!("Error handling control channel: {:?}", err);
                            });
                        trace!("AVDTP signaling session ended for 0x{:04x}", handle);
                        pending_streams.lock().remove(&handle);
                    })
                });
            }
            Some(pending) => match pending.take() {
                Some(sender) => {
                    trace!("Existing AVDTP session (transport channel)");
                    if channel.accept_connection().log_err().is_err() {
                        return;
                    }
                    spawn(async move {
                        if let Err(err) = channel.configure().await {
                            warn!("Error configuring channel: {:?}", err);
                            return;
                        }
                        sender
                            .send(channel)
                            .unwrap_or_else(|_| error!("Failed to send channel to session"));
                    });
                }
                None => {
                    warn!("Unexpected transport channel connection attempt");
                    channel
                        .reject_connection()
                        .ignore();
                }
            }
        }
    }
}

struct AvdtpSession {
    channel_sender: Arc<ChannelSender>,
    channel_receiver: OptionFuture<Receiver<Channel>>,
    local_endpoints: Arc<[LocalEndpoint]>,
    streams: Vec<Stream>
}

impl AvdtpSession {
    async fn handle_control_channel(&mut self, mut channel: Channel) -> Result<(), L2capError> {
        let mut assembler = SignalMessageAssembler::default();
        loop {
            select! {
                (i, _) = select_all(self.streams.iter_mut().map(Stream::process)) => {
                    debug!("Stream {} ended", i);
                    self.streams.swap_remove(i);
                },
                signal = channel.read() => match signal {
                    Some(packet) => match assembler.process_msg(packet) {
                        Ok(Some(header)) => {
                            let reply = self.handle_signal_message(header);
                            channel.send_signal(reply).await?;
                        }
                        Ok(None) => continue,
                        Err(err) => {
                            warn!("Error processing signaling message: {:?}", err);
                            continue;
                        }
                    },
                    None => break,
                },
                res = &mut self.channel_receiver => {
                    let channel = res.expect("Channel receiver closed");
                    self.streams
                        .iter_mut()
                        .find(|stream| stream.is_opening())
                        .map(|stream| stream.set_channel(channel))
                        .unwrap_or_else(|| warn!("No stream waiting for channel"));
                }
            }
        }
        Ok(())
    }

    fn get_endpoint(&self, seid: u8) -> Result<&LocalEndpoint, Error> {
        self.local_endpoints
            .iter()
            .find(|ep| ep.seid == seid)
            .ok_or(Error::BadAcpSeid)
    }

    fn get_stream(&mut self, seid: u8) -> Result<&mut Stream, Error> {
        #[allow(clippy::obfuscated_if_else)]
        self.streams
            .iter_mut()
            .find(|stream| stream.local_endpoint == seid)
            .ok_or_else(|| {
                self.local_endpoints
                    .iter()
                    .any(|ep| ep.seid == seid)
                    .then_some(Error::BadState)
                    .unwrap_or(Error::BadAcpSeid)
            })
    }

    fn handle_signal_message(&mut self, msg: SignalMessage) -> SignalMessage {
        assert_eq!(msg.message_type, MessageType::Command);
        let resp = SignalMessageResponse::for_msg(&msg);
        let mut data = msg.data;
        match msg.signal_identifier {
            // ([AVDTP] Section 8.6).
            SignalIdentifier::Discover => resp.try_accept((), |buf, _| {
                data.finish()?;
                trace!("Got DISCOVER request");
                for endpoint in self.local_endpoints.iter() {
                    buf.write(endpoint.as_stream_endpoint());
                }
                Ok(())
            }),
            // ([AVDTP] Section 8.7).
            SignalIdentifier::GetCapabilities => resp.try_accept((), |buf, _| {
                let seid = data.read_be::<u8>()? >> 2;
                data.finish()?;
                trace!("Got GET_CAPABILITIES request for 0x{:02x}", seid);
                let ep = self.get_endpoint(seid)?;
                ep.capabilities
                    .iter()
                    .filter(|cap| cap.is_basic())
                    .for_each(|cap| buf.write_ref(cap));
                Ok(())
            }),
            // ([AVDTP] Section 8.8).
            SignalIdentifier::GetAllCapabilities => resp.try_accept((), |buf, _| {
                let seid = data.read_be::<u8>()? >> 2;
                data.finish()?;
                trace!("Got GET_ALL_CAPABILITIES request for 0x{:02x}", seid);
                let ep = self.get_endpoint(seid)?;
                buf.write_ref(&ep.capabilities);
                Ok(())
            }),
            // ([AVDTP] Section 8.9).
            SignalIdentifier::SetConfiguration => resp.try_accept(ServiceCategory::Unknown, |_, _| {
                let acp_seid = data.read_be::<u8>()? >> 2;
                let int_seid = data.read_be::<u8>()? >> 2;
                let capabilities: Vec<Capability> = data.read_be()?;
                data.finish()?;
                trace!("Got SET_CONFIGURATION request for 0x{:02x} -> 0x{:02x}", acp_seid, int_seid);
                let ep = self.get_endpoint(acp_seid)?;
                ensure!(
                    self.streams
                        .iter()
                        .all(|stream| stream.local_endpoint != acp_seid),
                    Error::BadState
                );
                self.streams.push(Stream::new(ep, int_seid, capabilities)?);
                Ok(())
            }),
            // ([AVDTP] Section 8.10).
            SignalIdentifier::GetConfiguration => resp.try_accept((), |buf, _| {
                let seid = data.read_be::<u8>()? >> 2;
                data.finish()?;
                trace!("Got GET_CONFIGURATION request for 0x{:02x}", seid);
                let stream = self.get_stream(seid)?;
                buf.write_ref(stream.get_capabilities()?);
                Ok(())
            }),
            // ([AVDTP] Section 8.11).
            SignalIdentifier::Reconfigure => resp.try_accept(ServiceCategory::Unknown, |_, _| {
                let acp_seid = data.read_be::<u8>()? >> 2;
                let capabilities: Vec<Capability> = data.read_be()?;
                data.finish()?;
                trace!("Got RECONFIGURE request for 0x{:02x}", acp_seid);
                let ep = self
                    .local_endpoints
                    .iter()
                    .find(|ep| ep.seid == acp_seid)
                    .ok_or(Error::BadAcpSeid)?;
                let stream = self
                    .streams
                    .iter_mut()
                    .find(|stream| stream.local_endpoint == acp_seid)
                    .ok_or(Error::BadState)?;
                stream.reconfigure(capabilities, ep)?;
                Ok(())
            }),
            // ([AVDTP] Section 8.12).
            SignalIdentifier::Open => resp.try_accept((), |_, _| {
                let seid = data.read_be::<u8>()? >> 2;
                data.finish()?;
                trace!("Got OPEN request for 0x{:02x}", seid);
                let stream = self.get_stream(seid)?;
                stream.set_to_opening()?;
                let (tx, rx) = tokio::sync::oneshot::channel();
                self.channel_sender.set(Some(tx));
                self.channel_receiver.set(rx);
                Ok(())
            }),
            // ([AVDTP] Section 8.13).
            SignalIdentifier::Start => resp.try_accept(0x00u8, |_, ctx| {
                while {
                    let seid = data.read_be::<u8>()? >> 2;
                    data.finish()?;
                    *ctx = seid;
                    let sink = self.get_stream(seid)?;
                    trace!("Got START request for 0x{:02x}", seid);
                    sink.start()?;
                    !data.is_empty()
                } {}
                Ok(())
            }),
            // ([AVDTP] Section 8.14).
            SignalIdentifier::Close => resp.try_accept((), |_, _| {
                let seid = data.read_be::<u8>()? >> 2;
                data.finish()?;
                trace!("Got CLOSE request for 0x{:02x}", seid);
                let stream = self.get_stream(seid)?;
                stream.close()?;
                Ok(())
            }),
            // ([AVDTP] Section 8.15).
            SignalIdentifier::Suspend => resp.try_accept(0x00u8, |_, ctx| {
                while {
                    let seid = data.read_be::<u8>()? >> 2;
                    data.finish()?;
                    *ctx = seid;
                    trace!("Got SUSPEND request for 0x{:02x}", seid);
                    let sink = self.get_stream(seid)?;
                    sink.stop()?;
                    !data.is_empty()
                } {}
                Ok(())
            }),
            // ([AVDTP] Section 8.16).
            SignalIdentifier::Abort => resp.try_accept((), |_, _| {
                let seid = data.read_be::<u8>()? >> 2;
                data.finish()?;
                trace!("Got ABORT request for 0x{:02x}", seid);
                if let Some(id) = self
                    .streams
                    .iter_mut()
                    .position(|stream| stream.local_endpoint == seid)
                {
                    self.streams.swap_remove(id);
                }
                Ok(())
            }),
            // ([AVDTP] Section 8.17).
            SignalIdentifier::SecurityControl => resp.unsupported(),
            // ([AVDTP] Section 8.18).
            SignalIdentifier::Unknown => resp.general_reject(),
            // ([AVDTP] Section 8.19).
            SignalIdentifier::DelayReport => resp.unsupported()
        }
    }
}

struct SignalMessageResponse {
    transaction_label: u8,
    signal_identifier: SignalIdentifier
}

impl SignalMessageResponse {
    pub fn for_msg(msg: &SignalMessage) -> Self {
        Self {
            transaction_label: msg.transaction_label,
            signal_identifier: msg.signal_identifier
        }
    }

    pub fn general_reject(&self) -> SignalMessage {
        warn!("Unsupported signaling message: {:?}", self.signal_identifier);
        SignalMessage {
            transaction_label: self.transaction_label,
            message_type: MessageType::GeneralReject,
            signal_identifier: self.signal_identifier,
            data: Bytes::new()
        }
    }

    pub fn unsupported(&self) -> SignalMessage {
        self.try_accept((), |_, _| Err(Error::NotSupportedCommand))
    }

    pub fn try_accept<F, C>(&self, err_ctx: C, f: F) -> SignalMessage
    where
        F: FnOnce(&mut BytesMut, &mut C) -> Result<(), Error>,
        C: Instruct<BigEndian>
    {
        let mut buf = BytesMut::new();
        let mut ctx = err_ctx;
        match f(&mut buf, &mut ctx) {
            Ok(()) => SignalMessage {
                transaction_label: self.transaction_label,
                message_type: MessageType::ResponseAccept,
                signal_identifier: self.signal_identifier,
                data: buf.freeze()
            },
            Err(reason) => {
                warn!("Rejecting signal {:?} because of {:?}", self.signal_identifier, reason);
                buf.clear();
                buf.write_be(ctx);
                buf.write_be(reason);
                SignalMessage {
                    transaction_label: self.transaction_label,
                    message_type: MessageType::ResponseReject,
                    signal_identifier: self.signal_identifier,
                    data: buf.freeze()
                }
            }
        }
    }
}
