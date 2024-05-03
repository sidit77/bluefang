use std::collections::BTreeMap;
use std::sync::Arc;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use smallvec::SmallVec;
use tokio::{select, spawn};
use tokio::sync::mpsc::unbounded_channel;
use tracing::{debug, error, trace, warn};
use crate::{ensure, hci};
use crate::hci::acl::{AclDataAssembler, AclDataPacket};
use crate::hci::consts::{EventCode, LinkType, RemoteAddr, Status};
use crate::hci::{Error, Event, Hci};
use crate::utils::SliceExt;

const CID_ID_NONE: u16 = 0x0000;
const CID_ID_SIGNALING: u16 = 0x0001;

pub fn start_l2cap_server(hci: Arc<Hci>) -> Result<(), hci::Error> {
    let mut data = {
        let (tx, rx) = unbounded_channel();
        hci.register_data_handler(tx)?;
        rx
    };
    let mut events = {
        let (tx, rx) = unbounded_channel();
        hci.register_event_handler(
            [
                EventCode::ConnectionComplete,
                EventCode::DisconnectionComplete,
                EventCode::MaxSlotsChange
            ],
            tx)?;
        rx
    };
    spawn(async move {
        let mut state = State {
            hci,
            connections: Default::default(),
        };

        loop {
            select! {
                Some(event) = events.recv() => {
                    if let Err(err) = state.handle_event(event) {
                        warn!("Error handling event: {:?}", err);
                    }
                },
                Some(data) = data.recv() => {
                    if let Err(err) = state.handle_data(data) {
                        warn!("Error handling data: {:?}", err);
                    }
                },
                else => break,

            }
        }
        trace!("L2CAP server finished");
    });
    Ok(())
}

struct PhysicalConnection {
    handle: u16,
    max_slots: u8,
    addr: RemoteAddr,
    assembler: AclDataAssembler,
}

struct State {
    hci: Arc<Hci>,
    connections: BTreeMap<u16, PhysicalConnection>,
}

impl State {

    fn get_connection(&mut self, handle: u16) -> Result<&mut PhysicalConnection, Error> {
        self.connections.get_mut(&handle).ok_or(Error::UnknownConnectionHandle(handle))
    }

    fn handle_event(&mut self, event: Event) -> Result<(), Error> {
        let Event { code, mut data, .. } = event;
        match code {
            EventCode::ConnectionComplete => {
                // ([Vol 4] Part E, Section 7.7.3).
                let status = data.u8().map(Status::from)?;
                let handle = data.u16()?;
                let addr = data.array().map(RemoteAddr::from)?;
                let link_type = data.u8().map(LinkType::from)?;
                let _encryption_enabled = data.u8().map(|b| b == 0x01)?;
                data.finish()?;

                assert_eq!(link_type, LinkType::Acl);
                if status == Status::Success {
                    assert!(self
                        .connections
                        .insert(handle, PhysicalConnection {
                            handle,
                            max_slots: 0x01,
                            addr,
                            assembler: AclDataAssembler::default(),
                        }).is_none());
                    debug!("Connection complete: 0x{:04X} {}", handle, addr);
                } else {
                    warn!("Connection failed: {:?}", status);
                }
            },
            EventCode::DisconnectionComplete => {
                // ([Vol 4] Part E, Section 7.7.5).
                let status = data.u8().map(Status::from)?;
                let handle = data.u16()?;
                let reason = data.u8().map(Status::from)?;
                data.finish()?;

                self.connections.remove(&handle);
                if status == Status::Success {
                    debug!("Disconnection complete: {:?} {:?}", handle, reason);
                } else {
                    warn!("Disconnection failed: {:?}", status);
                }
            },
            EventCode::MaxSlotsChange => {
                // ([Vol 4] Part E, Section 7.7.27).
                let handle = data.u16()?;
                let max_slots = data.u8()?;
                data.finish()?;
                self.get_connection(handle)?.max_slots = max_slots;
                debug!("Max slots change: {:?} {:?}", handle, max_slots);
            }
            _ => unreachable!()
        }
        Ok(())
    }

    fn handle_data(&mut self, data: AclDataPacket) -> Result<(), Error> {
        debug!("ACL data: {:02X?}", data);
        let handle = data.handle;
        if let Some(pdu) = self.get_connection(handle)?.assembler.push(data) {
            self.handle_l2cap_packet(handle, &pdu)?;
        }
        Ok(())
    }

    // ([Vol 3] Part A, Section 3.1).
    fn handle_l2cap_packet(&mut self, handle: u16, data: &[u8]) -> Result<(), Error> {
        let len = u16::from_le_bytes(*data.get_chunk(0).ok_or(Error::BadEventPacketSize)?);
        let cid = u16::from_le_bytes(*data.get_chunk(2).ok_or(Error::BadEventPacketSize)?);
        let data = &data[4..];
        ensure!(data.len() == len as usize, Error::BadEventPacketSize);

        debug!("    L2CAP header: cid={:04X}", cid);
        // ([Vol 3] Part A, Section 2.1).
        match cid {
            CID_ID_NONE => Err(Error::BadEventPacketValue),
            CID_ID_SIGNALING => self.handle_l2cap_signaling(handle, data),
            _ => {
                warn!("Unhandled L2CAP CID: {:04X}", cid);
                Ok(())
            },
        }
    }

    // ([Vol 3] Part A, Section 4).
    fn handle_l2cap_signaling(&mut self, handle: u16, data: &[u8]) -> Result<(), Error> {
        // TODO: Send reject response when signal code or cid is unknown
        // TODO: Handle more than one command per packet
        let code = data.get(0)
            .ok_or(Error::BadEventPacketSize)
            .and_then(|c| SignalingCodes::try_from(*c)
                .map_err(|_| Error::BadEventPacketValue))?;
        let id = *data.get(1).ok_or(Error::BadEventPacketSize)?;
        let len = u16::from_le_bytes(*data.get_chunk(2).ok_or(Error::BadEventPacketSize)?) as usize;
        let data = &data[4..];
        ensure!(data.len() == len, Error::BadEventPacketSize);
        debug!("      L2CAP signaling: code={:?} id={:02X}", code, id);
        let reply = match code {
            SignalingCodes::InformationRequest => Some((SignalingCodes::InformationResponse, self.handle_information_request(data)?)),
            SignalingCodes::ConnectionRequest => Some((SignalingCodes::ConnectionResponse, self.handle_connection_request(data)?)),
            _ => {
                warn!("        Unsupported");
                // ([Vol 3] Part A, Section 4.1).
                let mut reply = ReplyPacket::default();
                reply.prepend(0x0000u16.to_le_bytes()); // Command not understood.
                Some((SignalingCodes::CommandReject, reply))
            },
        };
        if let Some((code, mut reply)) = reply {
            reply.prepend((reply.len() as u16).to_le_bytes());
            reply.prepend(id.to_le_bytes());
            reply.prepend(u8::from(code).to_le_bytes());
            let len = reply.len();
            reply.prepend(CID_ID_SIGNALING.to_le_bytes());
            reply.prepend((len as u16).to_le_bytes());
            self.hci.send_acl_data(handle, reply.as_ref())?;
        }
        Ok(())
    }

    // ([Vol 3] Part A, Section 4.10).
    fn handle_information_request(&mut self, data: &[u8]) -> Result<ReplyPacket, Error> {
        let info_type = u16::from_le_bytes(*data.get_chunk(0).ok_or(Error::BadEventPacketSize)?);
        let mut reply = ReplyPacket::default();
        match info_type {
            0x0001 => {
                debug!("        Connectionless MTU");
                let mtu = 512u16; //TODO: fill in the real value
                reply.prepend(mtu.to_le_bytes());
            }
            0x0002 => {
                debug!("        Local supported features");
                // ([Vol 3] Part A, Section 4.12).
                let mut features: u32 = 0;
                //features |= 1 << 3; // Enhanced Retransmission Mode
                //features |= 1 << 5; // FCS
                features |= 1 << 7; // Fixed Channels supported over BR/EDR
                features |= 1 << 9; // Unicast Connectionless Data Reception

                reply.prepend(features.to_le_bytes());
            },
            0x0003 => {
                debug!("        Fixed channels supported");
                // ([Vol 3] Part A, Section 4.13).
                let mut channels: u64 = 0;
                channels |= 1 << 1; // L2CAP Signaling channel
                channels |= 1 << 2; // Connectionless reception
                reply.prepend(channels.to_le_bytes());
            }
            _ => {
                error!("        Unknown information request: type={:04X}", info_type);
                return Err(Error::BadEventPacketValue);
            }
        }
        reply.prepend(0x0000u16.to_le_bytes());
        reply.prepend(info_type.to_le_bytes());
        Ok(reply)
    }

    // ([Vol 3] Part A, Section 4.2).
    fn handle_connection_request(&mut self, data: &[u8]) -> Result<ReplyPacket, Error> {

        let (psm, scid) = {
            let mut value = 0u64;
            let mut index = 0;
            loop {
                let octet = *data.get(index).ok_or(Error::BadEventPacketSize)?;
                value |= (octet as u64) << (index as u64 * 8);
                if octet & 0x01 == 0 {
                    break;
                }
                index += 1;
                assert!(index < 8, "PSM too long");
            }
            (value, u16::from_le_bytes(*data.get_chunk(index + 1).ok_or(Error::BadEventPacketSize)?))
        };
        debug!("        Connection request: PSM={:04X} SCID={:04X}", psm, scid);

        let dcid = 0x0080u16; //TODO: fill in the real value

        let mut reply = ReplyPacket::default();
        reply.prepend(0x0000u16.to_le_bytes()); // No further information available.
        reply.prepend(0x0000u16.to_le_bytes()); // Connection successful.
        reply.prepend(scid.to_le_bytes());
        reply.prepend(dcid.to_le_bytes());


        Ok(reply)
    }

}


// ([Vol 3] Part A, Section 4).
#[derive(Debug, Copy, Clone, Eq, PartialEq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
enum SignalingCodes {
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

#[derive(Default, Debug)]
pub struct ReplyPacket(SmallVec<[u8; 32]>);

impl ReplyPacket {

    pub fn prepend<const N: usize>(&mut self, data: [u8; N]) {
        self.0.insert_many(0, data);
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl AsRef<[u8]> for ReplyPacket {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

