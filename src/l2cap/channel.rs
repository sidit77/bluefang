use std::future::{poll_fn, Future};
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::{BufMut, Bytes, BytesMut};
use futures_lite::FutureExt;
use instructor::utils::Length;
use instructor::{BufferMut, Instruct, LittleEndian};
use tokio::sync::mpsc::UnboundedReceiver as MpscReceiver;
use tokio::time::sleep;
use tracing::{debug, info_span, instrument, trace, warn, Span, error};
use tracing::field::Empty;
use crate::ensure;

use crate::hci::{AclSendError, AclSender};
use crate::l2cap::configuration::{ConfigurationParameter, FlushTimeout, Mtu};
use crate::l2cap::signaling::{Psm, RejectReason, SignalingCode, SignalingContext};
use crate::l2cap::{ChannelEvent, CID_ID_NONE, ConfigureResult, ConnectionResult, ConnectionStatus, L2capHeader, SignalingIds};
use crate::utils::{now_or_never, Loggable, IgnoreableResult};

macro_rules! event {
    ($evt: expr) => {
        if let Some(evt) = $evt {
            return Poll::Ready(Ok(evt));
        }
    };
}

const DEFAULT_MTU: Mtu = Mtu(1691);

enum Event {
    DataReceived(Bytes),
    ConnectionComplete,
    ConfigurationCompete,
    DisconnectComplete
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("Failed to build packet: {0}")]
    InvalidData(#[from] instructor::Error),
    #[error("This action is not allowed in the current channel state")]
    BadState,
    #[error("The operation timed out")]
    Timeout,
    #[error("The channel has been disconnected")]
    Disconnected,
    #[error("The underlying transport has been closed. Is the event loop still running?")]
    ChannelClosed
}

impl From<AclSendError> for Error {
    fn from(value: AclSendError) -> Self {
        match value {
            AclSendError::EventLoopClosed => Self::ChannelClosed,
            AclSendError::InvalidData(e) => Self::InvalidData(e)
        }
    }
}

impl Loggable for Error {
    fn should_log(&self) -> bool {
        !matches!(self, Error::Disconnected | Error::ChannelClosed)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum State {
    Closed(ClosedState),
    WaitConnectRsp,
    Config(ConfigState),
    Open,
    WaitDisconnect
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum ClosedState {
    Idle,
    WaitingForResponse(u8),
    Disconnected
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum ConfigState {
    Config,
    SendConfig,
    ConfigReqRsp,
    ConfigRsp,
    ConfigReq
    //IndFinalRsp,
    //FinalRsp,
    //ControlInd
}

pub struct Channel {
    connection_handle: u16,
    state: State,
    remote_cid: u16,
    local_cid: u16,
    receiver: MpscReceiver<ChannelEvent>,
    sender: AclSender,
    next_signaling_id: SignalingIds,
    local_mtu: Mtu,
    remote_mtu: Mtu,
    flush_timeout: FlushTimeout,
    span: Span
}

impl Channel {

    pub fn new(connection_handle: u16, local_cid: u16, receiver: MpscReceiver<ChannelEvent>, sender: AclSender, next_signaling_id: SignalingIds) -> Self {
        Self {
            connection_handle,
            state: State::Closed(ClosedState::Idle),
            remote_cid: CID_ID_NONE,
            local_cid,
            receiver,
            sender,
            next_signaling_id,
            local_mtu: Mtu::MINIMUM_ACL_U,
            remote_mtu: Mtu::MINIMUM_ACL_U,
            flush_timeout: FlushTimeout::default(),
            span: info_span!(parent: None, "l2cap_channel", remote_cid = Empty, local_cid = format_args!("{:#X}", local_cid))
        }
    }

    pub fn is_response_pending(&self) -> bool {
        matches!(self.state, State::Closed(ClosedState::WaitingForResponse(_)))
    }

    pub fn connection_request_received(&mut self, remote_cid: u16, transaction_id: u8) {
        debug_assert!(self.state == State::Closed(ClosedState::Idle), "Connection request received in non-idle state");
        self.set_remote_cid(remote_cid);
        self.set_state(State::Closed(ClosedState::WaitingForResponse(transaction_id)));
    }

    #[instrument(parent = &self.span, skip(self))]
    pub async fn connect(&mut self, psm: u64) -> Result<(), Error> {
        ensure!(self.state == State::Closed(ClosedState::Idle), Error::BadState);
        self.send_signaling(None, SignalingCode::ConnectionRequest, (Psm(psm), self.local_cid))?;
        self.set_state(State::WaitConnectRsp);
        self.wait_for_connection().await?;
        Ok(())
    }

    #[instrument(parent = &self.span, skip(self))]
    pub fn accept_connection(&mut self) -> Result<(), Error> {
        if let State::Closed(ClosedState::WaitingForResponse(transaction_id)) = self.state {
            self.send_signaling(Some(transaction_id), SignalingCode::ConnectionResponse, (
                self.local_cid,
                self.remote_cid,
                ConnectionResult::Success,
                ConnectionStatus::NoFurtherInformation))?;
            self.set_state(State::Config(ConfigState::Config));
            Ok(())
        } else {
            Err(Error::BadState)
        }
    }

    #[instrument(parent = &self.span, skip(self))]
    pub fn reject_connection(&mut self) -> Result<(), Error> {
        if let State::Closed(ClosedState::WaitingForResponse(transaction_id)) = self.state {
            self.send_signaling(Some(transaction_id), SignalingCode::ConnectionResponse, (
                self.local_cid,
                self.remote_cid,
                ConnectionResult::RefusedNoResources,
                ConnectionStatus::NoFurtherInformation))?;
            self.set_state(State::Closed(ClosedState::Disconnected));
            Ok(())
        } else {
            Err(Error::BadState)
        }
    }

    fn set_remote_cid(&mut self, remote_cid: u16) {
        self.remote_cid = remote_cid;
        self.span.record("remote_cid", format_args!("{:#X}", remote_cid));
    }

    pub fn connection_handle(&self) -> u16 {
        self.connection_handle
    }

    pub fn remote_mtu(&self) -> u16 {
        self.remote_mtu.0
    }

    fn set_state(&mut self, state: State) -> Option<Event> {
        debug_assert_ne!(self.state, state, "State transition to same state");
        trace!("State transition: {:?} -> {:?}", self.state, state);
        self.state = state;
        match self.state {
            State::Closed(ClosedState::Disconnected) => Some(Event::DisconnectComplete),
            State::Open => Some(Event::ConfigurationCompete),
            State::Config(ConfigState::Config) => Some(Event::ConnectionComplete),
            _ => None
        }
    }

    pub fn read(&mut self) -> impl Future<Output = Option<Bytes>> + '_ {
        poll_fn(move |cx| self.poll_data(cx))
    }

    #[instrument(parent = &self.span, skip(self, data))]
    pub async fn write(&mut self, data: Bytes) -> Result<(), Error> {
        if self.state != State::Open {
            trace!("Channel not yet open, waiting for configuration");
            self.wait_for_configuration_complete()
                .or(timeout(Duration::from_secs(2)))
                .await?;
        }
        let mut buffer = BytesMut::new();
        buffer.write_le(L2capHeader {
            len: Length::new(data.len())?,
            cid: self.remote_cid
        });
        buffer.put(data);
        self.sender.send(self.connection_handle, buffer.freeze())?;
        Ok(())
    }

    #[instrument(parent = &self.span, skip(self))]
    pub async fn disconnect(&mut self) -> Result<(), Error> {
        self.send_signaling(None, SignalingCode::DisconnectionRequest, (self.remote_cid, self.local_cid))?;
        self.set_state(State::WaitDisconnect);
        self.wait_for_disconnect().await
    }

    #[instrument(parent = &self.span, skip(self))]
    pub async fn configure(&mut self) -> Result<(), Error> {
        match self.state {
            State::Config(ConfigState::Config) => self.set_state(State::Config(ConfigState::ConfigReqRsp)),
            State::Config(ConfigState::SendConfig) => self.set_state(State::Config(ConfigState::ConfigRsp)),
            State::Open => self.set_state(State::Config(ConfigState::ConfigReqRsp)),
            _ => return Err(Error::BadState)
        };
        // Send ConfigReq
        self.send_configuration_request(vec![DEFAULT_MTU.into()])?;
        self.local_mtu = DEFAULT_MTU;

        //self.wait_for_configuration_complete().await?;
        Ok(())
    }

    #[instrument(parent = &self.span, skip(self, cx))]
    fn poll_events(&mut self, cx: &mut Context<'_>) -> Poll<Result<Event, Error>> {
        use ChannelEvent::*;
        while let Poll::Ready(data) = self.receiver.poll_recv(cx) {
            let Some(data) = data else {
                return Poll::Ready(Err(Error::ChannelClosed));
            };
            match self.state {
                // ([Vol 3] Part A, Section 6.1.1)
                State::Closed(cs) => match data {
                    ConfigurationRequest { id, .. } => {
                        /* Send CommandReject (with reason Invalid CID) */
                        self.send_invalid_cid(id)?;
                    }
                    DisconnectRequest { id } => {
                        /* Send DisconnectRsp */
                        self.send_disconnect_response(id)?;
                        if cs != ClosedState::Disconnected {
                            event!(self.set_state(State::Closed(ClosedState::Disconnected)));
                        }
                    }
                    _ => { /* Ignore */ }
                },
                // ([Vol 3] Part A, Section 6.1.2)
                State::WaitConnectRsp => match data {
                    ConnectionResponse { result, remote_cid, .. } => match result {
                        ConnectionResult::Success => {
                            self.set_remote_cid(remote_cid);
                            event!(self.set_state(State::Config(ConfigState::Config)));
                        }
                        ConnectionResult::Pending => { /* Stall */ }
                        _ => {
                            event!(self.set_state(State::Closed(ClosedState::Disconnected)));
                        }
                    }
                    ConfigurationRequest { id, .. } => {
                        /* Send CommandReject (with reason Invalid CID) */
                        self.send_invalid_cid(id)?;
                    }
                    DataReceived(_) | ConfigurationResponse { .. } | DisconnectRequest { .. } | DisconnectResponse { .. } => { /* Ignore */  }
                }
                // ([Vol 3] Part A, Section 6.1.4)
                State::Config(cs) => match data {
                    ConfigurationRequest { id, options} => match cs {
                        ConfigState::Config => {
                            event!(self.handle_config_req(id, options, State::Config(ConfigState::SendConfig))?);
                        }
                        ConfigState::ConfigReqRsp => {
                            event!(self.handle_config_req(id, options, State::Config(ConfigState::ConfigRsp))?);
                        }
                        ConfigState::ConfigReq => {
                            event!(self.handle_config_req(id, options, State::Open)?);
                        }
                        _ => debug!("Unexpected ConfigurationRequest in state {:?}", self.state)
                    },
                    ConfigurationResponse {result, options, .. } => match cs {
                        ConfigState::ConfigReqRsp => {
                            event!(self.handle_config_resp(result, options, State::Config(ConfigState::ConfigReq))?);
                        }
                        ConfigState::ConfigRsp => {
                            event!(self.handle_config_resp(result, options, State::Open)?);
                        }
                        ConfigState::Config | ConfigState::SendConfig | ConfigState::ConfigReq => { /* Ignore */ }
                    },
                    DisconnectRequest { id } => {
                        // Send DisconnectRsp
                        self.send_disconnect_response(id)?;
                        event!(self.set_state(State::Closed(ClosedState::Disconnected)));
                    }
                    DisconnectResponse { .. } | ConnectionResponse { .. } => { /* Ignore */ }
                    DataReceived(data) => return Poll::Ready(Ok(Event::DataReceived(data)))
                },
                // ([Vol 3] Part A, Section 6.1.5)
                State::Open => match data {
                    ConfigurationRequest { id, options} => {
                        event!(self.handle_config_req(id, options, State::Config(ConfigState::SendConfig))?);
                    }
                    DisconnectRequest { id } => {
                        // Send DisconnectRsp
                        self.send_disconnect_response(id)?;
                        event!(self.set_state(State::Closed(ClosedState::Disconnected)));
                    }
                    DataReceived(data) => return Poll::Ready(Ok(Event::DataReceived(data))),
                    DisconnectResponse { .. } | ConfigurationResponse { .. } | ConnectionResponse { .. } => { /* Ignore */ }
                },
                // ([Vol 3] Part A, Section 6.1.6)
                State::WaitDisconnect => match data {
                    ConfigurationRequest { id, .. } => {
                        // Send CommandReject with reason Invalid CID
                        self.send_invalid_cid(id)?;
                    }
                    DisconnectRequest { id} => {
                        // Send DisconnectRsp
                        self.send_disconnect_response(id)?;
                        event!(self.set_state(State::Closed(ClosedState::Disconnected)));
                    }
                    DisconnectResponse { .. } => {
                        event!(self.set_state(State::Closed(ClosedState::Disconnected)));
                    }
                    DataReceived(_) | ConfigurationResponse { .. } | ConnectionResponse { .. } => { /* Ignore */ }
                }
            }
        }
        Poll::Pending
    }

    pub fn poll_data(&mut self, cx: &mut Context<'_>) -> Poll<Option<Bytes>> {
        while let Poll::Ready(event) = self.poll_events(cx) {
            match event {
                Ok(Event::DataReceived(data)) => return Poll::Ready(Some(data)),
                Ok(Event::DisconnectComplete) | Err(Error::Disconnected | Error::ChannelClosed | Error::Timeout) => return Poll::Ready(None),
                Ok(Event::ConnectionComplete | Event::ConfigurationCompete) => {}
                Err(e) => panic!("{}", e)
            }
        }
        Poll::Pending
    }

    fn handle_config_req(&mut self, id: u8, mut options: Vec<ConfigurationParameter>, success: State) -> Result<Option<Event>, Error> {
        let updated = false;
        for option in options.iter_mut() {
            match option {
                ConfigurationParameter::Mtu(mtu) => self.remote_mtu = *mtu,
                //TODO How to actually handle a flush timeout?
                ConfigurationParameter::FlushTimeout(timeout) => self.flush_timeout = *timeout,
                _ => {
                    warn!("Unsupported configuration parameter: {:?}", option);
                    self.send_configuration_response(id, ConfigureResult::Rejected, Vec::new())?;
                    return Ok(None);
                }
            }
        }
        if updated {
            self.send_configuration_response(id, ConfigureResult::UnacceptableParameters, options)?;
            Ok(None)
        } else {
            self.send_configuration_response(id, ConfigureResult::Success, options)?;
            Ok(self.set_state(success))
        }
    }

    fn handle_config_resp(&mut self, result: ConfigureResult, options: Vec<ConfigurationParameter>, success: State) -> Result<Option<Event>, Error> {
        match result {
            ConfigureResult::Success => {
                for option in options {
                    match option {
                        ConfigurationParameter::Mtu(mtu) => self.local_mtu = mtu,
                        _ => warn!("Unexpected configuration parameter: {:?}", option)
                    }
                }
                Ok(self.set_state(success))
            }
            other => {
                // Send ConfigReq (new options)
                unimplemented!("Configuration failed: {:?}", other)
            }
        }
    }

    fn wait_for_connection(&mut self) -> impl Future<Output = Result<(), Error>> + '_ {
        poll_fn(|cx| {
            if let State::Closed(ClosedState::Disconnected) = self.state {
                return Poll::Ready(Err(Error::Disconnected));
            }
            while let Poll::Ready(event) = self.poll_events(cx) {
                match event? {
                    Event::ConnectionComplete => return Poll::Ready(Ok(())),
                    Event::DisconnectComplete => return Poll::Ready(Err(Error::Disconnected)),
                    _ => warn!("Unexpected event")
                }
            }
            Poll::Pending
        })
    }

    fn wait_for_configuration_complete(&mut self) -> impl Future<Output = Result<(), Error>> + '_ {
        poll_fn(|cx| {
            if let State::Closed(ClosedState::Disconnected) = self.state {
                return Poll::Ready(Err(Error::Disconnected));
            }
            while let Poll::Ready(event) = self.poll_events(cx) {
                match event? {
                    Event::ConfigurationCompete => return Poll::Ready(Ok(())),
                    Event::DisconnectComplete => return Poll::Ready(Err(Error::Disconnected)),
                    Event::DataReceived(_) => warn!("Received data while still configuring"),
                    Event::ConnectionComplete => {}
                }
            }
            Poll::Pending
        })
    }

    fn wait_for_disconnect(&mut self) -> impl Future<Output = Result<(), Error>> + '_ {
        poll_fn(|cx| {
            if let State::Closed(ClosedState::Disconnected) = self.state {
                return Poll::Ready(Ok(()));
            }
            while let Poll::Ready(event) = self.poll_events(cx) {
                if let Event::DisconnectComplete = event? {
                    return Poll::Ready(Ok(()));
                }
            }
            Poll::Pending
        })
    }

    fn send_signaling<P: Instruct<LittleEndian>>(&self, id: Option<u8>, code: SignalingCode, parameters: P) -> Result<(), AclSendError> {
        self.sender.send_signaling(
            SignalingContext {
                handle: self.connection_handle,
                id: id.unwrap_or_else(|| self.next_signaling_id.next())
            },
            code,
            parameters
        )
    }

    fn send_disconnect_response(&self, id: u8) -> Result<(), AclSendError> {
        self.send_signaling(Some(id), SignalingCode::DisconnectionResponse, (self.local_cid, self.remote_cid))
    }

    fn send_invalid_cid(&self, id: u8) -> Result<(), AclSendError> {
        self.send_signaling(
            Some(id),
            SignalingCode::CommandReject,
            RejectReason::InvalidCid {
                scid: self.local_cid,
                dcid: self.remote_cid
            }
        )
    }

    fn send_configuration_request(&self, options: Vec<ConfigurationParameter>) -> Result<(), AclSendError> {
        self.send_signaling(None, SignalingCode::ConfigureRequest, (self.remote_cid, u16::MIN, options))
    }

    fn send_configuration_response(&self, id: u8, result: ConfigureResult, options: Vec<ConfigurationParameter>) -> Result<(), AclSendError> {
        self.send_signaling(Some(id), SignalingCode::ConfigureResponse, (self.remote_cid, u16::MIN, result, options))
    }
}

impl Drop for Channel {
    fn drop(&mut self) {
        if !matches!(self.state, State::Closed(_)) {
            // The first yield point should be after sending the disconnect message
            if let Some(result) = now_or_never(self.disconnect()) {
                result.ignore();
            }
        }
    }
}

async fn timeout(duration: Duration) -> Result<(), Error> {
    sleep(duration).await;
    Err(Error::Timeout)
}