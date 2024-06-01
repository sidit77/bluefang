use bytes::{BufMut, Bytes, BytesMut};
use instructor::{BigEndian, Buffer, BufferMut, Error, Exstruct, Instruct};
use instructor::utils::u24;
use crate::{ensure, log_assert};
use crate::avc::{CommandCode, Frame, Opcode, Subunit, SubunitType};

pub const PANEL: Subunit = Subunit { ty: SubunitType::Panel, id: 0 };
pub const BLUETOOTH_SIG_COMPANY_ID: u24 = u24::new(0x001958);

// ([AVRCP] Section 6.3)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Instruct, Exstruct)]
#[instructor(endian = "big")]
struct CommandHeader {
    pdu: Pdu,
    #[instructor(bitfield(u8))]
    #[instructor(bits(0..2))]
    packet_type: PacketType,
    parameter_length: u16,
}

// ([AVRCP] Section 6.3)
#[derive(Debug, Copy, Clone, PartialEq, Eq, Instruct, Exstruct)]
#[repr(u8)]
enum PacketType {
    Single = 0b00,
    Start = 0b01,
    Continue = 0b10,
    End = 0b11,
}

// ([AVRCP] Section 4.5)
#[derive(Debug, Copy, Clone, PartialEq, Eq, Instruct, Exstruct)]
#[repr(u8)]
pub enum Pdu {
    GetCapabilities = 0x10,

    ListPlayerApplicationSettingAttributes = 0x11,
    ListPlayerApplicationSettingValues = 0x12,
    GetCurrentPlayerApplicationSettingValue = 0x13,
    SetPlayerApplicationSettingValue = 0x14,
    GetPlayerApplicationSettingAttributeText = 0x15,
    GetPlayerApplicationSettingValueText = 0x16,
    InformDisplayableCharacterSet = 0x17,
    InformBatteryStatusOfCt = 0x18,

    GetElementAttributes = 0x20,

    GetPlayStatus = 0x30,
    RegisterNotification = 0x31,

    RequestContinuingResponse = 0x40,
    AbortContinuingResponse = 0x41,

    SetAbsoluteVolume = 0x50,

    SetAddressedPlayer = 0x60,

    SetBrowsedPlayer = 0x70,
    GetFolderItems = 0x71,
    ChangePath = 0x72,
    GetItemAttributes = 0x73,
    PlayItem = 0x74,
    GetTotalNumberOfItems = 0x75,

    Search = 0x80,
    AddToNowPlaying = 0x90,

    GeneralReject = 0xA0,
}

// ([AVRCP] Section 28)
#[derive(Debug, Copy, Clone, PartialEq, Eq, Instruct, Exstruct)]
#[repr(u8)]
pub enum Event {
    PlaybackStatusChanged = 0x00,
    TrackChanged = 0x02,
    TrackReachedEnd = 0x03,
    TrackReachedStart = 0x04,
    PlaybackPosChanged = 0x05,
    BatteryStatusChanged = 0x06,
    SystemStatusChanged = 0x07,
    PlayerApplicationSettingChanged = 0x08,
    NowPlayingContentChanged = 0x09,
    AvailablePlayerChanged = 0x0A,
    AddressedPlayerChanged = 0x0B,
    UidsChanged = 0x0C,
    VolumeChanged = 0x0D,
}

pub struct Command {
    pub pdu: Pdu,
    pub parameters: Bytes,
}

#[derive(Default)]
pub struct CommandAssembler {
    pdu: Option<Pdu>,
    data: BytesMut,
}

impl CommandAssembler {

    fn reset(&mut self) {
        self.data.clear();
        self.pdu = None;
    }

    pub fn process_msg(&mut self, mut packet: Bytes) -> Result<Option<Command>, Error> {
        let CommandHeader { pdu, packet_type, parameter_length } = packet.read()?;
        ensure!(parameter_length as usize == packet.len(), Error::InvalidValue);

        match packet_type {
            PacketType::Single => {
                log_assert!(self.pdu.is_none());
                self.reset();
                Ok(Some(Command { pdu, parameters: packet }))
            }
            PacketType::Start => {
                log_assert!(self.pdu.is_none());
                self.reset();
                self.pdu = Some(pdu);
                self.data.put(packet);
                Ok(None)
            }
            PacketType::Continue => {
                ensure!(self.pdu == Some(pdu), Error::InvalidValue);
                self.data.put(packet);
                Ok(None)
            }
            PacketType::End => {
                ensure!(self.pdu == Some(pdu), Error::InvalidValue);
                self.data.put(packet);
                let cmd = Command { pdu, parameters: self.data.split().freeze() };
                self.reset();
                Ok(Some(cmd))
            }
        }
    }
}

pub fn fragment_command<P, F, E>(cmd: CommandCode, pdu: Pdu, parameters: P, mut func: F) -> Result<(), E>
    where
        P: Instruct<BigEndian>,
        F: FnMut(Bytes) -> Result<(), E>
{
    const MAX_PAYLOAD_SIZE: usize = 512 - 3 - 3 - 3;
    let mut buffer = BytesMut::new();
    buffer.write(&parameters);
    let mut parameters = buffer.split().freeze();
    let mut first = true;
    while {
        buffer.write(&Frame {
            ctype: cmd,
            subunit: PANEL,
            opcode: Opcode::VendorDependent,
        });
        buffer.write_be(&BLUETOOTH_SIG_COMPANY_ID);
        let payload = parameters.split_to(MAX_PAYLOAD_SIZE.min(parameters.len()));
        let packet_type = match (first, parameters.is_empty()) {
            (true, true) => PacketType::Single,
            (true, false) => PacketType::Start,
            (false, false) => PacketType::Continue,
            (false, true) => PacketType::End,
        };
        buffer.write(&CommandHeader {
            pdu,
            packet_type,
            parameter_length: payload.len() as u16,
        });
        buffer.put(payload);
        func(buffer.split().freeze())?;
        first = false;
        !parameters.is_empty()
    }{}
    Ok(())
}

#[cfg(test)]
mod tests {
    use bytes::Buf;
    use crate::avc::CommandCode;
    use crate::avrcp::packets::{Event, fragment_command, Pdu};
    use crate::avrcp::Volume;

    #[test]
    pub fn test_fragmentation() {
        fragment_command(CommandCode::Interim, Pdu::RegisterNotification, (Event::VolumeChanged, Volume(0.0)), |data| {
            assert_eq!(&[0x0F, 0x48, 0x00, 0x00, 0x19, 0x58, 0x31, 0x00, 0x00, 0x02, 0x0D, 0x00], data.chunk());
            Ok::<(), ()>(())
        }).unwrap()
    }

}