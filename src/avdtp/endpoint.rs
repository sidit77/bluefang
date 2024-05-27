use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};
use bytes::Bytes;
use parking_lot::Mutex;
use tracing::debug;
use crate::avdtp::error::ErrorCode;
use crate::avdtp::packets::{MediaType, ServiceCategory, StreamEndpoint, StreamEndpointType};
use crate::ensure;
use crate::l2cap::channel::Channel;
use crate::l2cap::ChannelEvent;

pub type StreamHandlerFactory = Box<dyn Fn(&[(ServiceCategory, Bytes)]) -> Box<dyn StreamHandler> + Send + Sync + 'static>;

pub struct LocalEndpoint {
    pub media_type: MediaType,
    pub seid: u8,
    pub in_use: Arc<AtomicBool>,
    pub tsep: StreamEndpointType,
    pub capabilities: Vec<(ServiceCategory, Bytes)>,
    pub stream_handler_factory: StreamHandlerFactory,
}

impl LocalEndpoint {
    pub fn as_stream_endpoint(&self) -> StreamEndpoint {
        StreamEndpoint {
            seid: self.seid,
            in_use: self.in_use.load(Ordering::SeqCst),
            media_type: self.media_type,
            tsep: self.tsep,
        }
    }
}

enum StreamState {
    Configured,
    Opening,
    Open,
    Streaming,
    Closing,
    Aborting,
}

pub struct Stream {
    state: StreamState,
    endpoint_usage_lock: Arc<AtomicBool>,
    pub local_endpoint: u8,
    pub remote_endpoint: u8,
    pub capabilities: Vec<(ServiceCategory, Bytes)>,
    channel: Option<Channel>,
    handler: Mutex<Box<dyn StreamHandler>>
}

impl Stream {
    pub fn new(local_endpoint: &LocalEndpoint, remote_endpoint: u8, capabilities: Vec<(ServiceCategory, Bytes)>) -> Result<Self, ErrorCode> {
        ensure!(!local_endpoint.in_use.swap(true, Ordering::SeqCst), ErrorCode::SepInUse);
        let handler = (local_endpoint.stream_handler_factory)(&capabilities);
        Ok(Self {
            local_endpoint: local_endpoint.seid,
            remote_endpoint,
            state: StreamState::Configured,
            capabilities,
            channel: None,
            handler: Mutex::new(handler),
            endpoint_usage_lock: local_endpoint.in_use.clone(),
        })
    }

    pub fn set_to_opening(&mut self) {
        assert!(matches!(self.state, StreamState::Configured));
        assert!(self.channel.is_none());
        self.state = StreamState::Opening;
    }

    pub fn is_opening(&self) -> bool {
        matches!(self.state, StreamState::Opening)
    }

    pub fn set_channel(&mut self, channel: Channel) {
        assert!(matches!(self.state, StreamState::Opening));
        assert!(self.channel.is_none());
        self.channel = Some(channel);
        self.state = StreamState::Open;
    }

}

impl Drop for Stream {
    fn drop(&mut self) {
        self.endpoint_usage_lock.store(false, Ordering::SeqCst);
    }
}

impl Future for Stream {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.channel.as_mut() {
            Some(channel) => {
                match channel.receiver.poll_recv(cx) {
                    Poll::Ready(Some(e)) => {
                        if let ChannelEvent::DataReceived(data) = e {
                            let mut handler = self.handler.lock();
                            handler.on_data(data);
                        }
                        Poll::Pending
                    },
                    Poll::Ready(None) => {
                        self.state = StreamState::Closing;
                        Poll::Ready(())
                    },
                    Poll::Pending => Poll::Pending,
                }
            }
            None => match self.state {
                StreamState::Closing => Poll::Ready(()),
                _ => Poll::Pending,
            },
        }
    }
}

pub trait StreamHandler: Send + 'static {
    fn on_reconfigure(&mut self, capabilities: &[(ServiceCategory, Bytes)]);
    fn on_play(&mut self);
    fn on_stop(&mut self);

    fn on_data(&mut self, data: Bytes);
}


pub struct DebugStreamHandler;

impl StreamHandler for DebugStreamHandler {
    fn on_reconfigure(&mut self, capabilities: &[(ServiceCategory, Bytes)]) {
        debug!("Reconfigure: {:?}", capabilities);
    }

    fn on_play(&mut self) {
        debug!("Play");
    }

    fn on_stop(&mut self) {
        debug!("Stop");
    }

    fn on_data(&mut self, data: Bytes) {
        debug!("Data: {} bytes", data.len());
    }
}