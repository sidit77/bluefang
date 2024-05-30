use bytes::{BufMut, Bytes, BytesMut};
use instructor::{Buffer, Error, Exstruct, Instruct};

use crate::{ensure, log_assert};
use crate::sdp::Uuid;

// ([AVCTP] Section 6.1)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Instruct, Exstruct)]
#[instructor(endian = "big")]
struct PacketHeader {
    #[instructor(bitfield(u8))]
    #[instructor(bits(4..8))]
    transaction_label: u8,
    #[instructor(bits(2..4))]
    packet_type: PacketType,
    #[instructor(bits(0..2))]
    message_type: MessageType,
}

// ([AVCTP] Section 6.1)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Instruct, Exstruct)]
#[repr(u8)]
pub enum MessageType {
    Command = 0b00,
    Response = 0b10,
    ResponseInvalidProfile = 0b11,
}


// ([AVCTP] Section 6.1)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Instruct, Exstruct)]
#[repr(u8)]
enum PacketType {
    Single = 0b00,
    Start = 0b01,
    Continue = 0b10,
    End = 0b11,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub transaction_label: u8,
    pub profile_id: Uuid,
    pub message_type: MessageType,
    pub data: Bytes
}

#[derive(Default)]
pub struct MessageAssembler {
    data: BytesMut,
    transaction_label: u8,
    message_type: Option<MessageType>,
    profile_id: u16,
    num_packets: u8,
    packets_received: u8,
}

impl MessageAssembler {

    fn reset(&mut self) {
        self.data.clear();
        self.message_type = None;
        self.transaction_label = 0;
        self.num_packets = 0;
        self.packets_received = 0;
        self.profile_id = 0;
    }

    fn process_msg_internal(&mut self, mut data: Bytes) -> Result<Option<Message>, Error> {
        self.packets_received += 1;

        let PacketHeader { transaction_label, packet_type, message_type } = data.read()?;

        match packet_type {
            PacketType::Single => {
                log_assert!(self.message_type.is_none());
                self.reset();
                let profile_id: u16 = data.read_be()?;
                Ok(Some(Message { transaction_label, message_type, profile_id: Uuid::from_u16(profile_id), data }))
            }
            PacketType::Start => {
                log_assert!(self.message_type.is_none());
                self.reset();
                self.num_packets = data.read_be()?;
                self.profile_id = data.read_be()?;
                self.packets_received = 1;
                self.message_type = Some(message_type);
                self.transaction_label = transaction_label;
                self.data.put(data);
                Ok(None)
            }
            PacketType::Continue | PacketType::End => {
                let profile_id: u16 = data.read_be()?;
                ensure!(self.message_type.is_some(), Error::InvalidValue);
                ensure!(self.transaction_label == transaction_label, Error::InvalidValue);
                ensure!(self.profile_id == profile_id, Error::InvalidValue);
                ensure!(self.packets_received <= self.num_packets, Error::InvalidValue);
                self.data.put(data);
                match packet_type {
                    PacketType::End => {
                        ensure!(self.packets_received == self.num_packets, Error::InvalidValue);
                        let message = Message {
                            transaction_label: self.transaction_label,
                            message_type: self.message_type.unwrap(),
                            profile_id: Uuid::from_u16(self.profile_id),
                            data: self.data.split().freeze()
                        };
                        self.reset();
                        Ok(Some(message))
                    }
                    _ => Ok(None)
                }
            }
        }
    }

    pub fn process_msg(&mut self, data: Bytes) -> Result<Option<Message>, Error> {
        let result = self.process_msg_internal(data);
        if result.is_err() { self.reset(); }
        result
    }
}


#[cfg(test)]
mod test {
    use bytes::Bytes;

    use crate::avctp::packets::{Message, MessageAssembler, MessageType};
    use crate::sdp::Uuid;

    #[test]
    fn test_parse_packet() {
        let testdata: &[u8] = &[
            0x00, 0x11, 0x0E, 0x03, 0x48, 0x00, 0x00, 0x19, 0x58,
            0x31, 0x00, 0x00, 0x05, 0x0D, 0x00, 0x00, 0x00, 0x00,
        ];
        let data = Bytes::from_static(testdata);
        let mut assember: MessageAssembler = Default::default();
        assert_eq!(assember.process_msg(data).unwrap(), Some(Message {
            transaction_label: 0,
            profile_id: Uuid::from_u16(0x110E),
            message_type: MessageType::Command,
            data: Bytes::from_static(&testdata[3..])
        }));
    }

}