use bytes::{Bytes, BytesMut};
use instructor::{Buffer, BufferMut, Error, Exstruct, Instruct};
use tracing::warn;
use crate::{ensure, hci};
use crate::l2cap::channel::Channel;


// ([AVDTP] Section 8.4.2).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Exstruct, Instruct)]
#[repr(u8)]
enum PacketType {
    Single = 0b00,
    Start = 0b01,
    Continue = 0b10,
    End = 0b11,
}

// ([AVDTP] Section 8.4.3).
#[derive(Default, Debug, Copy, Clone, Eq, PartialEq, Exstruct, Instruct)]
#[repr(u8)]
pub enum MessageType {
    #[default]
    Command = 0b00,
    GeneralReject = 0b01,
    ResponseAccept = 0b10,
    ResponseReject = 0b11,
}

// ([AVDTP] Section 8.5).
#[derive(Default, Debug, Copy, Clone, Eq, PartialEq, Exstruct, Instruct)]
#[repr(u8)]
pub enum SignalIdentifier {
    //#[instructor(default)]
    //Unknown = 0x00,
    #[default]
    Discover = 0x01,
    GetCapabilities = 0x02,
    SetConfiguration = 0x03,
    GetConfiguration = 0x04,
    Reconfigure = 0x05,
    Open = 0x06,
    Start = 0x07,
    Close = 0x08,
    Suspend = 0x09,
    Abort = 0x0a,
    SecurityControl = 0x0b,
    GetAllCapabilities = 0x0c,
    DelayReport = 0x0d,
}

// ([AVDTP] Section 8.4).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Exstruct, Instruct)]
struct SignalHeader {
    #[instructor(bitfield(u8))]
    #[instructor(bits(4..8))]
    transaction_label: u8,
    #[instructor(bits(2..4))]
    packet_type: PacketType,
    #[instructor(bits(0..2))]
    message_type: MessageType,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Exstruct, Instruct)]
struct SignalIdentifierField {
    #[instructor(bitfield(u8))]
    #[instructor(bits(0..6))]
    signal_identifier: SignalIdentifier,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SignalMessage {
    pub transaction_label: u8,
    pub message_type: MessageType,
    pub signal_identifier: SignalIdentifier,
    pub data: Bytes,
}

#[derive(Default)]
pub struct SignalMessageAssembler {
    transaction_label: u8,
    message: BytesMut,
    message_type: MessageType,
    signal_identifier: SignalIdentifier,
    number_of_signaling_packets: u8,
    packet_count: u8,
}

impl SignalMessageAssembler {

    fn reset(&mut self) {
        self.transaction_label = 0;
        self.message.clear();
        self.message_type = MessageType::Command;
        self.signal_identifier = SignalIdentifier::Discover;
        self.number_of_signaling_packets = 0;
        self.packet_count = 0;
    }

    pub fn process_msg(&mut self, mut data: Bytes) -> Result<Option<SignalMessage>, Error> {
        self.packet_count += 1;

        let SignalHeader {transaction_label, packet_type, message_type} = data.read_be()?;

        match packet_type {
            PacketType::Single | PacketType::Start if !self.message.is_empty() => {
                warn!("Clearing incomplete message");
                self.reset();
            }
            PacketType::Continue | PacketType::End => {
                ensure!(self.transaction_label == transaction_label, Error::InvalidValue);
                ensure!(self.message_type == message_type, Error::InvalidValue);
            }
            _ => {}
        }
        match packet_type {
            PacketType::Single => {
                let signal_identifier = data.read_be::<SignalIdentifierField>()?.signal_identifier;
                Ok(Some(SignalMessage {
                    transaction_label,
                    message_type,
                    signal_identifier,
                    data
                }))
            },
            PacketType::Start => {
                self.transaction_label = transaction_label;
                self.message_type = message_type;
                self.number_of_signaling_packets = data.read_be()?;
                self.signal_identifier = data.read_be::<SignalIdentifierField>()?.signal_identifier;
                self.message.extend_from_slice(&data);
                Ok(None)
            },
            PacketType::Continue => match self.packet_count < self.number_of_signaling_packets {
                true => {
                    self.message.extend_from_slice(&data);
                    Ok(None)
                },
                false => {
                    warn!("Exceeded number of signaling packets (got: {}, expected: {})", self.packet_count, self.number_of_signaling_packets);
                    self.reset();
                    Err(Error::InvalidValue)
                }
            }
            PacketType::End => match self.packet_count == self.number_of_signaling_packets {
                true => {
                    self.message.extend_from_slice(&data);
                    let message = SignalMessage {
                        transaction_label: self.transaction_label,
                        message_type: self.message_type,
                        signal_identifier: self.signal_identifier,
                        data: self.message.split().freeze()
                    };
                    self.reset();
                    Ok(Some(message))
                },
                false => {
                    warn!("Insufficient number of signaling packets (got: {}, expected: {})", self.packet_count, self.number_of_signaling_packets);
                    self.reset();
                    Err(Error::InvalidValue)
                }

            }
        }
    }

}

pub trait SignalChannelExt {
    fn send_signal(&mut self, message: SignalMessage) -> Result<(), hci::Error>;
}

impl SignalChannelExt for Channel {
    fn send_signal(&mut self, SignalMessage { transaction_label, message_type, signal_identifier, data }: SignalMessage) -> Result<(), hci::Error> {
        let mut buffer = BytesMut::new();
        let (mut packet_type, chunk_size) = match data.len() + 2 <= self.remote_mtu as usize {
            true => (PacketType::Single, usize::MAX),
            false => (PacketType::Start, (self.remote_mtu - 2) as usize)
        };
        let number_of_signaling_packets = data.len().div_ceil(chunk_size);
        for (i, chunk) in data.chunks(chunk_size).enumerate() {
            buffer.write_be(&SignalHeader {
                transaction_label,
                packet_type,
                message_type,
            });
            if matches!(packet_type, PacketType::Start) {
                buffer.write_be(&u8::try_from(number_of_signaling_packets).expect("payload too large"));
            }
            if matches!(packet_type, PacketType::Single | PacketType::Start) {
                buffer.write_be(&SignalIdentifierField { signal_identifier });
            }
            buffer.extend_from_slice(chunk);
            self.write(buffer.split().freeze())?;
            packet_type = match i + 1 < number_of_signaling_packets {
                true => PacketType::Continue,
                false => PacketType::End
            };
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use crate::avdtp::packets::{SignalMessageAssembler};

    #[test]
    fn test_packets() {
        //let mut data = Bytes::from_static(&[0x00, 0x01]);
        let mut data = Bytes::from_static(&[0x12, 0x0c, 0x01, 0x00, 0x07, 0x06, 0x00, 0x00, 0xff, 0xff, 0x02, 0x35]);
        let mut assember = SignalMessageAssembler::default();
        println!("{:?}", assember.process_msg(data).unwrap());
    }
}