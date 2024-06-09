use bytes::{BufMut, Bytes, BytesMut};
use instructor::{Buffer, BufferMut, DoubleEndedBufferMut, Exstruct, Instruct, LittleEndian};
use instructor::utils::Length;
use tracing::{debug, error, warn};
use crate::{ensure, log_assert};
use crate::hci::Error;
use crate::l2cap::{ChannelEvent, CID_ID_SIGNALING, ConfigureResult, ConnectionResult, ConnectionStatus, L2capHeader, State};

#[derive(Debug, Copy, Clone)]
struct SignalingContext {
    handle: u16,
    id: u8,
}

impl State {
    fn send_response<F: FnOnce(&mut BytesMut)>(&self, ctx: SignalingContext, code: SignalingCodes, writer: F) -> Result<(), Error> {
        let mut data = BytesMut::new();
        writer(&mut data);
        data.write_front(SignalingHeader {
            code,
            id: ctx.id,
            length: Length::new(data.len())?,
        });
        data.write_front(L2capHeader {
            len: Length::new(data.len())?,
            cid: CID_ID_SIGNALING,
        });
        self.sender.send(ctx.handle, data.freeze())?;
        Ok(())
    }

    // ([Vol 3] Part A, Section 4).
    pub fn handle_l2cap_signaling(&mut self, handle: u16, mut data: Bytes) -> Result<(), Error> {
        // TODO: Send reject response when signal code or cid is unknown
        // TODO: Handle more than one command per packet
        let SignalingHeader { code, id, .. } = data.read()?;
        let ctx = SignalingContext { handle, id };
        debug!("      L2CAP signaling: code={:?} id={:02X}", code, id);
        match code {
            SignalingCodes::CommandReject => {
                let reason: RejectReason = data.read()?;
                data.finish()?;
                error!("        Command reject: {:?}", reason);
            },
            SignalingCodes::ConnectionRequest => self.handle_connection_request(ctx, data)?,
            SignalingCodes::ConfigureRequest => self.handle_configuration_request(ctx, data)?,
            SignalingCodes::ConfigureResponse => self.handle_configuration_response(ctx, data)?,
            SignalingCodes::DisconnectionRequest => self.handle_disconnect_request(ctx, data)?,
            SignalingCodes::EchoRequest => self.handle_echo_request(ctx, data)?,
            SignalingCodes::InformationRequest => self.handle_information_request(ctx, data)?,
            _ => {
                warn!("        Unsupported");
                self.send_response(ctx, SignalingCodes::CommandReject, |data| {
                    data.write_le(RejectReason::CommandNotUnderstood);
                })?;
            },
        };
        Ok(())
    }

     // ([Vol 3] Part A, Section 4.2).
    fn handle_connection_request(&mut self, ctx: SignalingContext, mut data: Bytes) -> Result<(), Error> {
        let psm: u64 = data.read_le::<Psm>()?.0;
        let scid: u16 = data.read_le()?;
        data.finish()?;
        let resp = self.handle_channel_connection(ctx.handle, psm, scid);

        self.send_response(ctx, SignalingCodes::ConnectionResponse, |data| {
            data.write_le(resp.ok().unwrap_or_default());
            data.write_le(scid);
            data.write_le(resp.err().unwrap_or(ConnectionResult::Success));
            data.write_le(ConnectionStatus::NoFurtherInformation);
        })?;
        Ok(())
    }

    // ([Vol 3] Part A, Section 4.4).
    fn handle_configuration_request(&mut self, ctx: SignalingContext, mut data: Bytes) -> Result<(), Error> {
        let dcid: u16 = data.read_le()?;
        let flags: u16 = data.read_le()?;
        //TODO handle continuation packets
        log_assert!(flags & 0xFFFE == 0);
        debug!("        Configuration request: DCID={:04X} flags={:04X}", dcid, flags);

        self.send_channel_msg(dcid, ChannelEvent::ConfigurationRequest(ctx.id, data))
    }

    // ([Vol 3] Part A, Section 4.5).
    fn handle_configuration_response(&mut self, ctx: SignalingContext, mut data: Bytes) -> Result<(), Error> {
        let scid: u16 = data.read_le()?;
        let flags: u16 = data.read_le()?;
        let result: ConfigureResult = data.read_le()?;
        //TODO handle continuation packets
        log_assert!(flags & 0xFFFE == 0);
        debug!("        Configuration response: SCID={:04X} flags={:04X}", scid, flags);

        self.send_channel_msg(scid, ChannelEvent::ConfigurationResponse(ctx.id, result, data))
    }

    // ([Vol 3] Part A, Section 4.6).
    fn handle_disconnect_request(&mut self, ctx: SignalingContext, mut data: Bytes) -> Result<(), Error> {
        let dcid: u16 = data.read_le()?;
        let scid: u16 = data.read_le()?;
        data.finish()?;
        debug!("        Disconnect request: DCID={:04X} SCID={:04X}", dcid, scid);
        match self.channels.remove(&dcid) {
            Some(_) => self.send_response(ctx, SignalingCodes::DisconnectionResponse, |data| {
                data.write_le(dcid);
                data.write_le(scid);
            })?,
            None => self.send_response(ctx, SignalingCodes::CommandReject, |data| {
                data.write_le(RejectReason::InvalidCid { scid, dcid });
            })?
        }
        Ok(())
    }

    fn handle_echo_request(&mut self, ctx: SignalingContext, data: Bytes) -> Result<(), Error> {
        self.send_response(ctx, SignalingCodes::EchoResponse, |resp| {
            resp.put(data);
        })
    }

    // ([Vol 3] Part A, Section 4.10).
    fn handle_information_request(&mut self, ctx: SignalingContext, mut data: Bytes) -> Result<(), Error> {
        let info_type: u16 = data.read_le()?;
        data.finish()?;
        self.send_response(ctx, SignalingCodes::InformationResponse, |data| {
            data.write_le(info_type);
            match info_type {
                0x0001 => {
                    debug!("        Connectionless MTU");
                    let mtu = 1024u16; //TODO: fill in the real value
                    data.write_le(0x0000u16); //Success
                    data.write_le(mtu);
                }
                0x0002 => {
                    debug!("        Local supported features");
                    // ([Vol 3] Part A, Section 4.12).
                    let mut features: u32 = 0;
                    //features |= 1 << 3; // Enhanced Retransmission Mode
                    //features |= 1 << 5; // FCS
                    features |= 1 << 7; // Fixed Channels supported over BR/EDR
                    //features |= 1 << 9; // Unicast Connectionless Data Reception

                    data.write_le(0x0000u16); //Success
                    data.write_le(features);
                },
                0x0003 => {
                    debug!("        Fixed channels supported");
                    // ([Vol 3] Part A, Section 4.13).
                    let mut channels: u64 = 0;
                    channels |= 1 << 1; // L2CAP Signaling channel
                    //channels |= 1 << 2; // Connectionless reception
                    //channels |= 1 << 7; // BR/EDR Security Manager

                    data.write_le(0x0000u16); //Success
                    data.write_le(channels);
                }
                _ => {
                    error!("        Unknown information request: type={:04X}", info_type);
                    data.write_le(0x0001u16); //Not supported
                }
            }
        })?;
        Ok(())
    }

}


// ([Vol 3] Part A, Section 4).
#[derive(Debug, Exstruct, Instruct)]
#[instructor(endian = "little")]
pub struct SignalingHeader {
    pub code: SignalingCodes,
    pub id: u8,
    pub length: Length<u16, 0>
}

// ([Vol 3] Part A, Section 4).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Exstruct, Instruct)]
#[repr(u8)]
pub enum SignalingCodes {
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
enum RejectReason {
    CommandNotUnderstood,
    SignalingMtuExceeded {
        actual_mtu: u16
    },
    InvalidCid {
        scid: u16,
        dcid: u16
    },
}

impl Instruct<LittleEndian> for RejectReason {
    fn write_to_buffer<B: BufferMut + ?Sized>(&self, buffer: &mut B) {
        match *self {
            RejectReason::CommandNotUnderstood => {
                buffer.write_le(0x0000u16);
            },
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
            },
            0x0002 => {
                let scid: u16 = buffer.read_le()?;
                let dcid: u16 = buffer.read_le()?;
                Ok(RejectReason::InvalidCid { scid, dcid })
            },
            _ => Err(instructor::Error::InvalidValue)
        }
    }
}

