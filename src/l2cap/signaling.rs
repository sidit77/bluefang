use tracing::{debug, error, warn};
use crate::{ensure, log_assert};
use crate::hci::Error;
use crate::l2cap::{CID_ID_SIGNALING, ReplyPacket, SignalingCodes, State};
use crate::utils::SliceExt;

impl State {
    // ([Vol 3] Part A, Section 4).
    pub fn handle_l2cap_signaling(&mut self, handle: u16, data: &[u8]) -> Result<(), Error> {
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

    // ([Vol 3] Part A, Section 4.4).
    fn handle_configuration_request(&mut self, data: &[u8]) -> Result<ReplyPacket, Error> {
        let dcid = u16::from_le_bytes(*data.get_chunk(0).ok_or(Error::BadEventPacketSize)?);
        let flags = u16::from_le_bytes(*data.get_chunk(2).ok_or(Error::BadEventPacketSize)?);
        //TODO handle continuation packets
        log_assert!(flags & 0xFFFE == 0);
        debug!("        Configuration request: DCID={:04X} flags={:04X}", dcid, flags);
        let mut reply = ReplyPacket::default();
        reply.prepend(0x0000u16.to_le_bytes()); // No further information available.
        reply.prepend(0x0000u16.to_le_bytes()); // Success.
        reply.prepend(dcid.to_le_bytes());
        Ok(reply)
    }



    // ([Vol 3] Part A, Section 4.10).
    fn handle_information_request(&mut self, data: &[u8]) -> Result<ReplyPacket, Error> {
        let info_type = u16::from_le_bytes(*data.get_chunk(0).ok_or(Error::BadEventPacketSize)?);
        let mut reply = ReplyPacket::default();
        match info_type {
            0x0001 => {
                debug!("        Connectionless MTU");
                let mtu = 1024u16; //TODO: fill in the real value
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

}