use std::panic::Location;
use bytes::{Bytes, BytesMut};
use instructor::utils::Length;
use instructor::{Buffer, BufferMut, Exstruct, Instruct, LittleEndian};
use tokio::sync::mpsc::unbounded_channel;
use tracing::{debug, error, instrument, Span, trace, warn};

use crate::hci::{AclSender, AclSendError, Error};
use crate::l2cap::{ChannelEvent, ConfigureResult, ConnectionResult, ConnectionStatus, L2capHeader, State, CID_ID_SIGNALING};
use crate::{ensure, log_assert};
use crate::l2cap::configuration::ConfigurationParameter;
use crate::utils::{catch_error, ResultExt};

#[derive(Debug, Copy, Clone)]
pub struct SignalingContext {
    pub handle: u16,
    pub id: u8
}

impl AclSender {

    pub fn send_signaling<P: Instruct<LittleEndian>>(&self, ctx: SignalingContext, code: SignalingCode, parameters: P) -> Result<(), AclSendError> {
        let mut data = BytesMut::new();
        data.write(parameters);
        let parameters = data.split().freeze();
        data.write(L2capHeader {
            len: Length::new(parameters.len() + 4)?,
            cid: CID_ID_SIGNALING
        });
        data.write(SignalingHeader {
            code,
            id: ctx.id,
            length: u16::try_from(parameters.len()).expect("Length overflow")
        });
        data.write_le(parameters);
        trace!(?code, id = ctx.id, "Sending signaling command");
        self.send(ctx.handle, data.freeze())
    }

}

impl State {
    //fn send_response<F: FnOnce(&mut BytesMut)>(&self, ctx: SignalingContext, code: SignalingCode, writer: F) -> Result<(), Error> {
    //    let mut data = BytesMut::new();
    //    writer(&mut data);
    //    data.write_front(SignalingHeader {
    //        code,
    //        id: ctx.id,
    //        length: Length::new(data.len())?
    //    });
    //    data.write_front(L2capHeader {
    //        len: Length::new(data.len())?,
    //        cid: CID_ID_SIGNALING
    //    });
    //    self.sender.send(ctx.handle, data.freeze())?;
    //    Ok(())
    //}

    // ([Vol 3] Part A, Section 4).
    #[instrument(skip(self, data))]
    pub fn handle_l2cap_signaling(&mut self, handle: u16, mut data: Bytes) -> Result<(), Error> {
        // TODO: Send reject response when cid is unknown
        while !data.is_empty() {
            let SignalingHeader { code, id, length } = data.read()?;
            Span::current()
                .record("code", format_args!("{:?}", code))
                .record("id", id);
            let mut data = data.split_to(length as usize);

            let ctx = SignalingContext { handle, id };
            let result = catch_error(|| match code {
                SignalingCode::CommandReject => {
                    let reason: RejectReason = data.read()?;
                    data.finish()?;
                    error!("Command rejected: {:?}", reason);
                    Ok(())
                }
                SignalingCode::ConnectionRequest => self.handle_connection_request(ctx, data),
                SignalingCode::ConfigureRequest => self.handle_configuration_request(ctx, data),
                SignalingCode::ConfigureResponse => self.handle_configuration_response(ctx, data),
                SignalingCode::DisconnectionRequest => self.handle_disconnect_request(ctx, data),
                SignalingCode::DisconnectionResponse => self.handle_disconnect_response(ctx, data),
                SignalingCode::EchoRequest => self.handle_echo_request(ctx, data),
                SignalingCode::InformationRequest => self.handle_information_request(ctx, data),
                _ => {
                    warn!("Command Unsupported");
                    Err(RejectReason::CommandNotUnderstood)
                }
            });
            if let Err(reason) = result {
                self.sender.send_signaling(ctx, SignalingCode::CommandReject, reason).ignore()
            }
        }
        Ok(())
    }

    // ([Vol 3] Part A, Section 4.2).
    fn handle_connection_request(&mut self, ctx: SignalingContext, mut data: Bytes) -> Result<(), RejectReason> {
        let psm: u64 = data.read_le::<Psm>()?.0;
        let scid: u16 = data.read_le()?;
        data.finish()?;
        let (tx, rx) = unbounded_channel();
        let resp = self.handle_channel_connection(ctx.handle, psm, scid, rx);

        self.sender.send_signaling(ctx, SignalingCode::ConnectionResponse, (
            resp.ok().unwrap_or_default(),
            scid,
            resp.err().unwrap_or(ConnectionResult::Success),
            ConnectionStatus::NoFurtherInformation,
        )).unwrap_or_else(|err| warn!("Failed to send connection response: {:?}", err));
        let _ = tx.send(ChannelEvent::OpenChannelResponseSent(resp.is_ok()));
        if let Ok(dcid) = resp {
            self.channels.insert(dcid, (scid, tx));
        }
        Ok(())
    }

    // ([Vol 3] Part A, Section 4.4).
    fn handle_configuration_request(&mut self, ctx: SignalingContext, mut data: Bytes) -> Result<(), RejectReason> {
        let dcid: u16 = data.read_le()?;
        let flags: u16 = data.read_le()?;
        //TODO handle continuation packets
        log_assert!(flags & 0xFFFE == 0);
        let param: Vec<ConfigurationParameter> = data.read()?;
        data.finish()?;
        debug!("Configuration request: DCID={:04X}", dcid);

        self.send_channel_msg(dcid, ChannelEvent::ConfigurationRequest(ctx.id, param))
            .map_err(|_| RejectReason::InvalidCid { scid: 0, dcid })
    }

    // ([Vol 3] Part A, Section 4.5).
    fn handle_configuration_response(&mut self, ctx: SignalingContext, mut data: Bytes) -> Result<(), RejectReason> {
        let scid: u16 = data.read_le()?;
        let flags: u16 = data.read_le()?;
        let result: ConfigureResult = data.read_le()?;
        //TODO handle continuation packets
        log_assert!(flags & 0xFFFE == 0);
        let param: Vec<ConfigurationParameter> = data.read()?;
        data.finish()?;
        debug!("Configuration response: SCID={:04X}", scid);

        self.send_channel_msg(scid, ChannelEvent::ConfigurationResponse(ctx.id, result, param))
            .map_err(|_| RejectReason::InvalidCid { scid, dcid: 0 })
    }

    // ([Vol 3] Part A, Section 4.6).
    fn handle_disconnect_request(&mut self, ctx: SignalingContext, mut data: Bytes) -> Result<(), RejectReason> {
        let dcid: u16 = data.read_le()?;
        let scid: u16 = data.read_le()?;
        data.finish()?;
        debug!("Disconnect request: DCID={:04X} SCID={:04X}", dcid, scid);
        match self.channels.remove(&dcid) {
            Some((_, channel)) => {
                let _ = channel.send(ChannelEvent::DisconnectRequest(ctx.id));
                Ok(())
            },
            None => Err(RejectReason::InvalidCid { scid, dcid })
        }
    }

    // ([Vol 3] Part A, Section 4.7).
    fn handle_disconnect_response(&mut self, ctx: SignalingContext, mut data: Bytes) -> Result<(), RejectReason> {
        let dcid: u16 = data.read_le()?;
        let scid: u16 = data.read_le()?;
        data.finish()?;
        debug!("Disconnect response: DCID={:04X} SCID={:04X}", dcid, scid);
        match self.channels.remove(&dcid) {
            Some((_, channel)) => {
                let _ = channel.send(ChannelEvent::DisconnectResponse(ctx.id));
                Ok(())
            },
            None => {
                warn!("Channel not found DCID: {:04X}", dcid);
                Ok(())
            }
        }
    }

    // ([Vol 3] Part A, Section 4.8).
    fn handle_echo_request(&mut self, ctx: SignalingContext, data: Bytes) -> Result<(), RejectReason> {
        self.sender.send_signaling(ctx, SignalingCode::EchoResponse, data).ignore();
        Ok(())
    }

    // ([Vol 3] Part A, Section 4.10).
    fn handle_information_request(&mut self, ctx: SignalingContext, mut data: Bytes) -> Result<(), RejectReason> {
        const SUCCESS: u16 = 0x0000;
        const NOT_SUPPORTED: u16 = 0x0001;
        let info_type: u16 = data.read_le()?;
        data.finish()?;
        match info_type {
            0x0001 => {
                debug!("Connectionless MTU");
                let mtu = 1024u16; //TODO: fill in the real value
                self.sender.send_signaling(ctx, SignalingCode::InformationResponse, (info_type, SUCCESS, mtu)).ignore();
            }
            0x0002 => {
                debug!("Local supported features");
                // ([Vol 3] Part A, Section 4.12).
                let mut features: u32 = 0;
                //features |= 1 << 3; // Enhanced Retransmission Mode
                //features |= 1 << 5; // FCS
                features |= 1 << 7; // Fixed Channels supported over BR/EDR
                //features |= 1 << 9; // Unicast Connectionless Data Reception

                self.sender.send_signaling(ctx, SignalingCode::InformationResponse, (info_type, SUCCESS, features)).ignore();
            }
            0x0003 => {
                debug!("        Fixed channels supported");
                // ([Vol 3] Part A, Section 4.13).
                let mut channels: u64 = 0;
                channels |= 1 << 1; // L2CAP Signaling channel
                //channels |= 1 << 2; // Connectionless reception
                //channels |= 1 << 7; // BR/EDR Security Manager

                self.sender.send_signaling(ctx, SignalingCode::InformationResponse, (info_type, SUCCESS, channels)).ignore();
            }
            _ => {
                error!("Unknown information request: type={:04X}", info_type);
                self.sender.send_signaling(ctx, SignalingCode::InformationResponse, (info_type, NOT_SUPPORTED)).ignore();
            }
        }
        Ok(())
    }
}

// ([Vol 3] Part A, Section 4).
#[derive(Debug, Exstruct, Instruct)]
#[instructor(endian = "little")]
pub struct SignalingHeader {
    pub code: SignalingCode,
    pub id: u8,
    pub length: u16
}

// ([Vol 3] Part A, Section 4).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Exstruct, Instruct)]
#[repr(u8)]
pub enum SignalingCode {
    CommandReject = 0x01,
    ConnectionRequest = 0x02,
    ConnectionResponse = 0x03,
    ConfigureRequest = 0x04,
    ConfigureResponse = 0x05,
    DisconnectionRequest = 0x06,
    DisconnectionResponse = 0x07,
    EchoRequest = 0x08,
    EchoResponse = 0x09,
    InformationRequest = 0x0A,
    InformationResponse = 0x0B,
    ConnectionParameterUpdateRequest = 0x12,
    ConnectionParameterUpdateResponse = 0x13,
    LECreditBasedConnectionRequest = 0x14,
    LECreditBasedConnectionResponse = 0x15,
    FlowControlCreditIndex = 0x16,
    CreditBasedConnectionRequest = 0x17,
    CreditBasedConnectionResponse = 0x18,
    CreditBasedReconfigurationRequest = 0x19,
    CreditBasedReconfigurationResponse = 0x1A,
    #[instructor(default)]
    Unknown = 0xFF
}

// ([Vol 3] Part A, Section 4.2).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
struct Psm(pub u64);

impl Exstruct<LittleEndian> for Psm {
    fn read_from_buffer<B: Buffer + ?Sized>(buffer: &mut B) -> Result<Self, instructor::Error> {
        let mut value = 0u64;
        let mut index = 0;
        loop {
            let octet: u8 = buffer.read_le()?;
            value |= (octet as u64) << (index as u64 * 8);
            if octet & 0x01 == 0 {
                break;
            }
            index += 1;
            ensure!(index < 8, instructor::Error::InvalidValue);
        }
        Ok(Self(value))
    }
}

// ([Vol 3] Part A, Section 4.1).
#[allow(dead_code)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum RejectReason {
    CommandNotUnderstood,
    SignalingMtuExceeded { actual_mtu: u16 },
    InvalidCid { scid: u16, dcid: u16 }
}

impl Instruct<LittleEndian> for RejectReason {
    fn write_to_buffer<B: BufferMut + ?Sized>(&self, buffer: &mut B) {
        match *self {
            RejectReason::CommandNotUnderstood => {
                buffer.write_le(0x0000u16);
            }
            RejectReason::SignalingMtuExceeded { actual_mtu } => {
                buffer.write_le(0x0001u16);
                buffer.write_le(actual_mtu);
            }
            RejectReason::InvalidCid { scid, dcid } => {
                buffer.write_le(0x0002u16);
                buffer.write_le(scid);
                buffer.write_le(dcid);
            }
        }
    }
}

impl Exstruct<LittleEndian> for RejectReason {
    fn read_from_buffer<B: Buffer + ?Sized>(buffer: &mut B) -> Result<Self, instructor::Error> {
        let reason: u16 = buffer.read_le()?;
        match reason {
            0x0000 => Ok(RejectReason::CommandNotUnderstood),
            0x0001 => {
                let actual_mtu: u16 = buffer.read_le()?;
                Ok(RejectReason::SignalingMtuExceeded { actual_mtu })
            }
            0x0002 => {
                let scid: u16 = buffer.read_le()?;
                let dcid: u16 = buffer.read_le()?;
                Ok(RejectReason::InvalidCid { scid, dcid })
            }
            _ => Err(instructor::Error::InvalidValue)
        }
    }
}

impl From<instructor::Error> for RejectReason {
    #[track_caller]
    fn from(err: instructor::Error) -> Self {
        debug!("Failed to parse data ({:?}) at {}", err, Location::caller());
        Self::CommandNotUnderstood
    }
}