use bytes::{Bytes, BytesMut};
use instructor::{Buffer, BufferMut, DoubleEndedBufferMut, Exstruct, Instruct, LittleEndian};
use instructor::utils::Length;
use tracing::{debug, error, warn};
use crate::{ensure, log_assert};
use crate::hci::Error;
use crate::l2cap::{CID_ID_SIGNALING, ConnectionResult, ConnectionStatus, L2capHeader, Server, State};

impl State {
    // ([Vol 3] Part A, Section 4).
    pub fn handle_l2cap_signaling(&mut self, handle: u16, mut data: Bytes) -> Result<(), Error> {
        // TODO: Send reject response when signal code or cid is unknown
        // TODO: Handle more than one command per packet
        let SignalingHeader { code, id, .. } = data.read()?;
        debug!("      L2CAP signaling: code={:?} id={:02X}", code, id);
        let reply = match code {
            SignalingCodes::InformationRequest => Some((SignalingCodes::InformationResponse, self.handle_information_request(data)?)),
            SignalingCodes::ConnectionRequest => Some((SignalingCodes::ConnectionResponse, self.handle_connection_request(data)?)),
            _ => {
                warn!("        Unsupported");
                // ([Vol 3] Part A, Section 4.1).
                let mut reply = BytesMut::new();
                reply.write_le(&0x0000u16); // Command not understood.
                Some((SignalingCodes::CommandReject, reply))
            },
        };
        if let Some((code, mut reply)) = reply {
            reply.write_front(&SignalingHeader {
                code,
                id,
                length: Length::with_offset(reply.len())?,
            });
            reply.write_front(&L2capHeader {
                len: Length::with_offset(reply.len())?,
                cid: CID_ID_SIGNALING,
            });
            self.hci.send_acl_data(handle, reply.freeze())?;
        }
        Ok(())
    }

     // ([Vol 3] Part A, Section 4.2).
    fn handle_connection_request(&mut self, mut data: Bytes) -> Result<BytesMut, Error> {
         let psm: u64 = data.read_le::<Psm>()?.0;
         let scid: u16 = data.read_le()?;
         let resp = self.handle_channel_connection(psm, scid);

         let mut reply = BytesMut::new();
         reply.write_le(&resp.ok().unwrap_or_default());
         reply.write_le(&scid);
         reply.write_le(&resp.err().unwrap_or(ConnectionResult::Success));
         reply.write_le(&ConnectionStatus::NoFurtherInformation);

         Ok(reply)
    }
//
    //    let (psm, scid) = {
    //        let mut value = 0u64;
    //        let mut index = 0;
    //        loop {
    //            let octet = *data.get(index).ok_or(Error::BadEventPacketSize)?;
    //            value |= (octet as u64) << (index as u64 * 8);
    //            if octet & 0x01 == 0 {
    //                break;
    //            }
    //            index += 1;
    //            assert!(index < 8, "PSM too long");
    //        }
    //        (value, u16::from_le_bytes(*data.get_chunk(index + 1).ok_or(Error::BadEventPacketSize)?))
    //    };
    //    debug!("        Connection request: PSM={:04X} SCID={:04X}", psm, scid);
//
    //    let dcid = 0x0080u16; //TODO: fill in the real value
//
    //    let mut reply = ReplyPacket::default();
    //    reply.prepend(0x0000u16.to_le_bytes()); // No further information available.
    //    reply.prepend(0x0000u16.to_le_bytes()); // Connection successful.
    //    reply.prepend(scid.to_le_bytes());
    //    reply.prepend(dcid.to_le_bytes());
//
//
    //    Ok(reply)
    //}

    // ([Vol 3] Part A, Section 4.4).
    //fn handle_configuration_request(&mut self, data: Bytes) -> Result<BytesMut, Error> {
    //    let dcid = u16::from_le_bytes(*data.get_chunk(0).ok_or(Error::BadEventPacketSize)?);
    //    let flags = u16::from_le_bytes(*data.get_chunk(2).ok_or(Error::BadEventPacketSize)?);
    //    //TODO handle continuation packets
    //    log_assert!(flags & 0xFFFE == 0);
    //    debug!("        Configuration request: DCID={:04X} flags={:04X}", dcid, flags);
    //    let mut reply = BytesMut::new();
    //
    //    reply.prepend(0x0000u16.to_le_bytes()); // No further information available.
    //    reply.prepend(0x0000u16.to_le_bytes()); // Success.
    //    reply.prepend(dcid.to_le_bytes());
    //    Ok(reply)
    //}



    // ([Vol 3] Part A, Section 4.10).
    fn handle_information_request(&mut self, mut data: Bytes) -> Result<BytesMut, Error> {
        let info_type: u16 = data.read_le()?;
        data.finish()?;
        let mut reply = BytesMut::new();
        match info_type {
            0x0001 => {
                debug!("        Connectionless MTU");
                let mtu = 1024u16; //TODO: fill in the real value
                reply.write_front::<_, LittleEndian>(&mtu);
            }
            0x0002 => {
                debug!("        Local supported features");
                // ([Vol 3] Part A, Section 4.12).
                let mut features: u32 = 0;
                //features |= 1 << 3; // Enhanced Retransmission Mode
                //features |= 1 << 5; // FCS
                features |= 1 << 7; // Fixed Channels supported over BR/EDR
                features |= 1 << 9; // Unicast Connectionless Data Reception

                reply.write_front::<_, LittleEndian>(&features);
            },
            0x0003 => {
                debug!("        Fixed channels supported");
                // ([Vol 3] Part A, Section 4.13).
                let mut channels: u64 = 0;
                channels |= 1 << 1; // L2CAP Signaling channel
                channels |= 1 << 2; // Connectionless reception
                reply.write_front::<_, LittleEndian>(&channels);
            }
            _ => {
                error!("        Unknown information request: type={:04X}", info_type);
                return Err(Error::BadPacket(instructor::Error::InvalidValue));
            }
        }
        reply.write_front::<_, LittleEndian>(&0x0000u16);
        reply.write_front::<_, LittleEndian>(&info_type);
        Ok(reply)
    }

}


// ([Vol 3] Part A, Section 4).
#[derive(Debug, Exstruct, Instruct)]
#[instructor(endian = "little")]
struct SignalingHeader {
    code: SignalingCodes,
    id: u8,
    length: Length<u16, 0>
}

// ([Vol 3] Part A, Section 4).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Exstruct, Instruct)]
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