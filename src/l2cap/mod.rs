use num_enum::{IntoPrimitive, TryFromPrimitive};
use nusb::transfer::{Queue, RequestBuffer, TransferError};
use smallvec::SmallVec;
use tracing::{debug, error, warn};
use crate::ensure;
use crate::utils::SliceExt;

pub async fn do_l2cap(mut acl_in: Queue<RequestBuffer>, mut acl_out: Queue<Vec<u8>>) {
    loop {
        let data = acl_in.next_complete().await;
        match data.status {
            Ok(_) => match handle_acl_packet(&data.data) {
                Ok(None) => {}
                Ok(Some(reply)) => {
                    debug!("Sending reply...");
                    //TODO wait if there are too many packets in the queue
                    acl_out.submit(reply.as_ref().to_vec());
                },
                Err(err) => warn!("Error handling ACL packet: {:?}", err),
            },
            Err(err) => warn!("Error reading ACL data: {:?}", err),
        }
        let len = data.data.capacity();
        acl_in.submit(RequestBuffer::reuse(data.data, len));
    }
}

// ([Vol 4] Part E, Section 5.4.2).
pub fn handle_acl_packet(data: &[u8]) -> Result<Option<ReplyPacket>, Error> {
    debug!("ACL packet: {:02X?}", data);
    let hdr = u16::from_le_bytes(*data.get_chunk(0).ok_or(Error::BadPacket)?);
    let handle = hdr & 0xFFF;
    let pb = (hdr >> 12) & 0b11;
    let bc = (hdr >> 14) & 0b11;
    let len = u16::from_le_bytes(*data.get_chunk(2).ok_or(Error::BadPacket)?) as usize;
    let data = &data[4..];
    ensure!(data.len() == len, Error::BadPacket);

    debug!("  ACL header: handle={:X} pb={:02b} bc={:02b}", handle, pb, bc);
    Ok(handle_l2cap_packet(data)?
        .map(|mut reply| {
            let len = reply.len() as u16;
            reply.prepend(len.to_le_bytes());
            reply.prepend(handle.to_le_bytes());
            reply
        }))

}

// ([Vol 3] Part A, Section 3.1).
fn handle_l2cap_packet(data: &[u8]) -> Result<Option<ReplyPacket>, Error> {
    let len = u16::from_le_bytes(*data.get_chunk(0).ok_or(Error::BadPacket)?);
    let cid = u16::from_le_bytes(*data.get_chunk(2).ok_or(Error::BadPacket)?);
    let data = &data[4..];
    ensure!(data.len() == len as usize, Error::BadPacket);

    debug!("    L2CAP header: cid={:04X}", cid);
    // ([Vol 3] Part A, Section 2.1).
    match cid {
        0x0000 => Err(Error::BadPacket),
        0x0001 => {
            Ok(handle_l2cap_signaling(data)?
                .map(|mut reply| {
                    let len = reply.len() as u16;
                    reply.prepend(cid.to_le_bytes());
                    reply.prepend(len.to_le_bytes());
                    reply
                }))
        },
        _ => {
            warn!("Unhandled L2CAP CID: {:04X}", cid);
            Ok(None)
        },
    }
}

// ([Vol 3] Part A, Section 4).
fn handle_l2cap_signaling(data: &[u8]) -> Result<Option<ReplyPacket>, Error> {
    // TODO: Send reject response when signal code or cid is unknown
    let code = data.get(0)
        .and_then(|c| SignalingCodes::try_from(*c).ok())
        .ok_or(Error::BadPacket)?;
    let id = *data.get(1).ok_or(Error::BadPacket)?;
    let len = u16::from_le_bytes(*data.get_chunk(2).ok_or(Error::BadPacket)?) as usize;
    let data = &data[4..];
    ensure!(data.len() == len, Error::BadPacket);
    debug!("      L2CAP signaling: code={:?} id={:02X}", code, id);
    let reply = match code {
        SignalingCodes::InformationRequest => Some((SignalingCodes::InformationResponse, handle_information_request(data)?)),
        SignalingCodes::ConnectionRequest => Some((SignalingCodes::ConnectionResponse, handle_connection_request(data)?)),
        _ => {
            warn!("        Unsupported");
            None
        },
    };
    Ok(reply.map(|(code, mut reply)| {
        reply.prepend((reply.len() as u16).to_le_bytes());
        reply.prepend(id.to_le_bytes());
        reply.prepend(u8::from(code).to_le_bytes());
        reply
    }))
}

// ([Vol 3] Part A, Section 4.10).
fn handle_information_request(data: &[u8]) -> Result<ReplyPacket, Error> {
    let info_type = u16::from_le_bytes(*data.get_chunk(0).ok_or(Error::BadPacket)?);
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
            return Err(Error::BadPacket);
        }
    }
    reply.prepend(u8::from(true).to_le_bytes());
    reply.prepend(info_type.to_le_bytes());
    Ok(reply)
}

// ([Vol 3] Part A, Section 4.2).
fn handle_connection_request(data: &[u8]) -> Result<ReplyPacket, Error> {

    let (psm, scid) = {
        let mut value = 0u64;
        let mut index = 0;
        loop {
            let octet = *data.get(index).ok_or(Error::BadPacket)?;
            value |= (octet as u64) << (index as u64 * 8);
            if octet & 0x01 == 0 {
                break;
            }
            index += 1;
            assert!(index < 8, "PSM too long");
        }
        (value, u16::from_le_bytes(*data.get_chunk(index + 1).ok_or(Error::BadPacket)?))
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

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("{0}")]
    Generic(&'static str),
    #[error(transparent)]
    TransportError(#[from] nusb::Error),
    #[error(transparent)]
    TransferError(#[from] TransferError),
    #[error("Malformed packet")]
    BadPacket,
}