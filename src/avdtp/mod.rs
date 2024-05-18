mod packets;
mod error;

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Stdin, Write};
use std::process::{Command, Stdio};
use std::sync::Arc;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use instructor::{Buffer, BufferMut};
use tokio::spawn;
use tracing::{info, trace, warn};
use crate::avdtp::error::ErrorCode;
use crate::avdtp::packets::{AudioCodec, MediaType, MessageType, ServiceCategory, SignalChannelExt, SignalIdentifier, SignalMessage, SignalMessageAssembler, StreamEndpoint, StreamEndpointType};
use crate::hci::Error;
use crate::l2cap::channel::Channel;
use crate::l2cap::Server;
use crate::utils::MutexCell;

struct Sink {
    id: u8,
    media_type: MediaType,
    capabilities: Vec<(ServiceCategory, Bytes)>
}

pub struct AvdtpServer {
    sessions: BTreeMap<u16, Arc<AvdtpSession>>,
    sinks: Arc<Vec<Sink>>
}

impl Default for AvdtpServer {
    fn default() -> Self {
        Self {
            sessions: BTreeMap::new(),
            sinks: Arc::new(vec![
                Sink {
                    id: 1,
                    media_type: MediaType::Audio,
                    capabilities: vec![
                        (ServiceCategory::MediaTransport, Bytes::new()),
                        (ServiceCategory::MediaCodec, {
                            let mut codec = BytesMut::new();
                            codec.put_u8((MediaType::Audio as u8) << 4);
                            codec.put_u8(AudioCodec::Sbc as u8);
                            codec.put_slice(&[0xff, 0xff, 0x02, 0x35]);
                            codec.freeze()
                        }),
                    ]
                },
            ])
        }
    }
}

impl Server for AvdtpServer {
    fn on_connection(&mut self, mut channel: Channel) {
        let handle = channel.connection_handle;
        match self.sessions.get(&handle) {
            None => {
                trace!("New AVDTP session (signaling channel)");
                let session = Arc::new(AvdtpSession {
                    sinks: self.sinks.clone(),
                    next_connection: MutexCell::new(None),
                });
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
                //TODO removed the session from the map if the signal channel exits
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
    sinks: Arc<Vec<Sink>>,
    next_connection: MutexCell<Option<u8>>,
}

impl AvdtpSession {

    async fn handle_control_channel(&self, mut channel: Channel) -> Result<(), Error> {
        let mut assembler = SignalMessageAssembler::default();
        while let Some(packet) = channel.read().await {
            match assembler.process_msg(packet) {
                Ok(Some(header)) => {
                    let reply = self.handle_signal_message(header);
                    channel.send_signal(reply)?;
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

    async fn handle_transport_channel(&self, mut channel: Channel) {
        let seid = self.next_connection.take().unwrap();
        info!("New AVDTP transport channel for 0x{:02x}", seid);

        let mut sink = File::create("output.sbc").unwrap();
        while let Some(packet) = channel.read().await {
            info!("Received {} bytes on transport channel for seid 0x{:02x}", packet.len(), seid);
            sink.write_all(&packet[13..]).expect("Failed to write to ffplay");
        }
    }

    fn handle_signal_message(&self, msg: SignalMessage) -> SignalMessage {
        assert_eq!(msg.message_type, MessageType::Command);
        let resp = SignalMessageResponse::for_msg(&msg);
        let mut data = msg.data;
        match msg.signal_identifier {
            // ([AVDTP] Section 8.6).
            SignalIdentifier::Discover => resp.try_accept(|buf| {
                data.finish()?;
                for sink in self.sinks.iter() {
                    buf.write(&StreamEndpoint {
                        media_type: sink.media_type,
                        seid: sink.id,
                        in_use: false,
                        tsep: StreamEndpointType::Sink,
                    });
                }
                Ok(())
            }),
            // ([AVDTP] Section 8.7).
            SignalIdentifier::GetCapabilities => resp.general_reject(),
            // ([AVDTP] Section 8.8).
            SignalIdentifier::GetAllCapabilities => resp.try_accept(|buf| {
                let seid = data.read_be::<u8>()? >> 2;
                data.finish()?;
                let sink = self.sinks.iter()
                    .find(|sink| sink.id == seid)
                    .ok_or(ErrorCode::BadAcpSeid)?;
                for (category, caps) in sink.capabilities.iter() {
                    buf.write_be(category);
                    buf.write_be(&u8::try_from(caps.len()).expect("Capabilities to big"));
                    buf.write_be(caps);
                }
                Ok(())
            }),
            // ([AVDTP] Section 8.9).
            SignalIdentifier::SetConfiguration => resp.try_accept(|_| {
                //TODO add the required parameters to a reject
                let acp_seid = data.read_be::<u8>()? >> 2;
                let int_seid = data.read_be::<u8>()? >> 2;
                while !data.is_empty() {
                    let service: ServiceCategory = data.read_be()?;
                    info!("SET CONFIG (0x{:02x} -> 0x{:02x}): {:?}", int_seid, acp_seid, service);
                    let length: u8 = data.read_be()?;
                    data.advance(length as usize);
                }
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
                let sink = self.sinks.iter()
                    .find(|sink| sink.id == seid)
                    .ok_or(ErrorCode::BadAcpSeid)?;
                info!("OPEN (0x{:02x}): {:?}", seid, sink.media_type);
                self.next_connection.set(Some(seid));
                Ok(())
            }),
            // ([AVDTP] Section 8.13).
            SignalIdentifier::Start => resp.try_accept(|_| {
                //TODO handle rejects correctly
                while {
                    let seid = data.read_be::<u8>()? >> 2;
                    data.finish()?;
                    let sink = self.sinks.iter()
                        .find(|sink| sink.id == seid)
                        .ok_or(ErrorCode::BadAcpSeid)?;
                    info!("START (0x{:02x}): {:?}", seid, sink.media_type);
                    !data.is_empty()
                } {}
                Ok(())
            }),
            // ([AVDTP] Section 8.14).
            SignalIdentifier::Close => resp.try_accept(|_| {
                let seid = data.read_be::<u8>()? >> 2;
                data.finish()?;
                let sink = self.sinks.iter()
                .find(|sink| sink.id == seid)
                .ok_or(ErrorCode::BadAcpSeid)?;
                info!("CLOSE (0x{:02x}): {:?}", seid, sink.media_type);
                Ok(())
            }),
            // ([AVDTP] Section 8.15).
            SignalIdentifier::Suspend => resp.try_accept(|_| {
                //TODO handle rejects correctly
                while {
                    let seid = data.read_be::<u8>()? >> 2;
                    data.finish()?;
                    let sink = self.sinks.iter()
                        .find(|sink| sink.id == seid)
                        .ok_or(ErrorCode::BadAcpSeid)?;
                    info!("SUSPEND (0x{:02x}): {:?}", seid, sink.media_type);
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

    pub fn accept<F: FnOnce(&mut BytesMut)>(&self, f: F) -> SignalMessage {
        SignalMessage {
            transaction_label: self.transaction_label,
            message_type: MessageType::ResponseAccept,
            signal_identifier: self.signal_identifier,
            data: {
                let mut buf = BytesMut::new();
                f(&mut buf);
                buf.freeze()
            },
        }
    }

}
