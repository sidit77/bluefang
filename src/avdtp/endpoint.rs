use std::future::{poll_fn, Future};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::fmt::Debug;

use bytes::Bytes;
use tracing::warn;

use crate::avdtp::capabilities::Capability;
use crate::avdtp::error::Error;
use crate::avdtp::packets::{MediaType, StreamEndpoint, StreamEndpointType};
use crate::ensure;
use crate::l2cap::channel::Channel;


pub struct StreamHandlerFactory(Box<dyn Fn(&[Capability]) -> Box<dyn StreamHandler> + Send + Sync>);

impl StreamHandlerFactory {
    pub fn new<F, H>(factory: F) -> Self
        where
            F: Fn(&[Capability]) -> H + Send + Sync + 'static,
            H: StreamHandler
    {
        Self(Box::new(move |cap| Box::new(factory(cap))))
    }

    fn make_stream_handler(&self, capabilities: &[Capability]) -> Box<dyn StreamHandler> {
        (self.0)(capabilities)
    }
}

impl Debug for StreamHandlerFactory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StreamHandlerFactory(...)")
    }
}

#[derive(Debug)]
pub struct LocalEndpoint {
    pub media_type: MediaType,
    pub seid: u8,
    pub in_use: Arc<AtomicBool>,
    pub tsep: StreamEndpointType,
    pub capabilities: Vec<Capability>,
    pub factory: StreamHandlerFactory
}

impl LocalEndpoint {
    pub fn as_stream_endpoint(&self) -> StreamEndpoint {
        StreamEndpoint {
            seid: self.seid,
            in_use: self.in_use.load(Ordering::SeqCst),
            media_type: self.media_type,
            tsep: self.tsep
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum StreamState {
    Configured,
    Opening,
    Open,
    Streaming,
    Closing //Aborting,
}

pub struct Stream {
    state: StreamState,
    endpoint_usage_lock: Arc<AtomicBool>,
    pub local_endpoint: u8,
    pub remote_endpoint: u8,
    capabilities: Vec<Capability>,
    channel: Option<Channel>,
    handler: Box<dyn StreamHandler>
}

impl Stream {
    pub fn new(local_endpoint: &LocalEndpoint, remote_endpoint: u8, capabilities: Vec<Capability>) -> Result<Self, Error> {
        ensure!(!local_endpoint.in_use.swap(true, Ordering::SeqCst), Error::SepInUse);
        let handler = local_endpoint.factory.make_stream_handler(&capabilities);
        Ok(Self {
            local_endpoint: local_endpoint.seid,
            remote_endpoint,
            state: StreamState::Configured,
            capabilities,
            channel: None,
            handler,
            endpoint_usage_lock: local_endpoint.in_use.clone()
        })
    }

    pub fn reconfigure(&mut self, capabilities: Vec<Capability>, ep: &LocalEndpoint) -> Result<(), Error> {
        assert_eq!(self.local_endpoint, ep.seid);
        ensure!(matches!(self.state, StreamState::Open), Error::BadState);
        self.handler = ep.factory.make_stream_handler(&capabilities);
        self.capabilities = capabilities;
        Ok(())
    }

    pub fn set_to_opening(&mut self) -> Result<(), Error> {
        ensure!(matches!(self.state, StreamState::Configured), Error::BadState);
        ensure!(self.channel.is_none(), Error::BadState);
        self.state = StreamState::Opening;
        Ok(())
    }

    pub fn start(&mut self) -> Result<(), Error> {
        ensure!(matches!(self.state, StreamState::Open), Error::BadState);
        self.handler.on_play();
        self.state = StreamState::Streaming;
        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), Error> {
        ensure!(matches!(self.state, StreamState::Streaming), Error::BadState);
        self.handler.on_stop();
        self.state = StreamState::Open;
        Ok(())
    }

    pub fn close(&mut self) -> Result<(), Error> {
        ensure!(matches!(self.state, StreamState::Streaming | StreamState::Open), Error::BadState);
        if self.state == StreamState::Streaming {
            self.handler.on_stop();
        }
        self.state = StreamState::Closing;
        self.channel = None;
        Ok(())
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

    pub fn get_capabilities(&self) -> Result<&Vec<Capability>, Error> {
        ensure!(self.state != StreamState::Closing, Error::BadState);
        Ok(&self.capabilities)
    }

    pub fn process(&mut self) -> impl Future<Output = ()> + '_ {
        poll_fn(move |cx| self.poll(cx))
    }

    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        loop {
            match self.channel.as_mut() {
                Some(channel) => {
                    match channel.poll_data(cx) {
                        Poll::Ready(Some(data)) => {
                            if self.state == StreamState::Streaming {
                                //TODO Parse the realtime media header and do something useful with it
                                self.handler.on_data(data.slice(12..));
                            } else {
                                warn!("Data received while not streaming");
                            }
                        }
                        Poll::Ready(None) => {
                            self.state = StreamState::Closing;
                            self.channel = None;
                            return Poll::Ready(());
                        }
                        Poll::Pending => return Poll::Pending
                    }
                }
                None => {
                    return match self.state {
                        StreamState::Closing => Poll::Ready(()),
                        _ => Poll::Pending
                    };
                }
            }
        }
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        self.endpoint_usage_lock.store(false, Ordering::SeqCst);
    }
}

pub trait StreamHandler: 'static {
    fn on_play(&mut self);
    fn on_stop(&mut self);

    fn on_data(&mut self, data: Bytes);
}
