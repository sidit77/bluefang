use instructor::{BigEndian, Buffer, BufferMut, Error, Exstruct, Instruct};
use crate::ensure;

// ([AVC] Section 7.1)
#[derive(Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
#[instructor(endian = "big")]
struct Frame {
    #[instructor(bitfield(u8))]
    #[instructor(bits(0..4))]
    ctype: CommandCodes,
    subunit: Subunit,
    opcode: Opcode,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Instruct, Exstruct)]
#[repr(u8)]
pub enum CommandCodes {
    // ([AVC] Section 7.3.1)
    Control = 0x00,
    Status = 0x01,
    SpecificInquiry = 0x02,
    Notify = 0x03,
    GeneralInquiry = 0x04,

    // ([AVC] Section 7.3.2)
    NotImplemented = 0x08,
    Accepted = 0x09,
    Rejected = 0x0A,
    InTransition = 0x0B,
    Implemented = 0x0C,
    Changed = 0x0D,
    Interim = 0x0F,
}

impl CommandCodes {
    pub fn is_response(self) -> bool {
        self as u8 >= 0x08
    }
}

// ([AVC] Table 7.4)
#[derive(Debug, Copy, Clone, PartialEq, Eq, Instruct, Exstruct)]
#[repr(u8)]
pub enum SubunitType {
    Monitor = 0x00,
    Audio = 0x01,
    Printer = 0x02,
    Disc = 0x03,
    TapeRecorder = 0x04,
    Tuner = 0x05,
    CA = 0x06,
    Camera = 0x07,
    Panel = 0x09,
    BulletinBoard = 0x0A,
    CameraStorage = 0x0B,
    VendorUnique = 0x1C,
    Extended = 0x1E,
    Unit = 0x1F,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Instruct, Exstruct)]
#[repr(u8)]
pub enum Opcode {
    // 0x00 - 0x0f: unit and subunit commands
    VendorDependent = 0x00,
    Reserve = 0x01,
    PlugInfo = 0x02,

    // 0x10 - 0x3f: unit commands
    DigitalOutput = 0x10,
    DigitalInput = 0x11,
    ChannelUsage = 0x12,
    OutputPlugSignalFormat = 0x18,
    InputPlugSignalFormat = 0x19,
    GeneralBusSetup = 0x1f,
    ConnectAv = 0x20,
    DisconnectAv = 0x21,
    Connections = 0x22,
    Connect = 0x24,
    Disconnect = 0x25,
    UnitInfo = 0x30,
    SubunitInfo = 0x31,

    // 0x40 - 0x7f: subunit commands
    PassThrough = 0x7c,
    GuiUpdate = 0x7d,
    PushGuiData = 0x7e,
    UserAction = 0x7f,

    // 0xa0 - 0xbf: unit and subunit commands
    Version = 0xb0,
    Power = 0xb2
}

#[derive(Copy, Clone, Instruct, Exstruct)]
struct SubunitHeader {
    #[instructor(bitfield(u8))]
    #[instructor(bits(3..8))]
    ty: SubunitType,
    #[instructor(bits(0..3))]
    id: u8,
}

// ([AVC] Section 7.3.4)
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Subunit {
    pub ty: SubunitType,
    pub id: u32
}

impl Exstruct<BigEndian> for Subunit {
    fn read_from_buffer<B: Buffer>(buffer: &mut B) -> Result<Self, Error> {
        let SubunitHeader { ty, id} = buffer.read_be()?;
        //TODO support this
        ensure!(ty != SubunitType::Extended, Error::InvalidValue);
        ensure!(id != 6, Error::InvalidValue);

        let mut id = id as u32;
        if id == 5 {
            let extension: u8 = buffer.read_be()?;
            ensure!(extension != 0, Error::InvalidValue);
            if extension == 0xFF {
                id = (id + buffer.read_be::<u8>()? as u32) - 1;
            }
            id += extension as u32;
        }

        Ok(Self { ty, id })
    }
}

impl Instruct<BigEndian> for Subunit {
    fn write_to_buffer<B: BufferMut>(&self, buffer: &mut B) {
        assert_ne!(self.ty, SubunitType::Extended);
        assert!(self.id <= 514 && self.id != 5 && self.id != 6);
        let id = self.id.min(5) as u8;
        buffer.write_be(&SubunitHeader { ty: self.ty, id });
        let rem = self.id - id as u32;
        if rem > 0 {
            let extension = rem.min(0xFF) as u8;
            buffer.write_be(&extension);
            let rem = rem - extension as u32;
            if extension == 0xFF {
                buffer.write_be(&(rem as u8 + 1));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bytes::{Buf, Bytes, BytesMut};
    use instructor::{Buffer, BufferMut};
    use crate::avc::{CommandCodes, Frame, Opcode, Subunit, SubunitType};

    #[test]
    fn subunit_parsing() {
        let testcases: [(_, &[u8]); 3] = [
            (Subunit { ty: SubunitType::Monitor, id: 003 }, &[0b011]),
            (Subunit { ty: SubunitType::Monitor, id: 007 }, &[0b101, 0b00000010]),
            (Subunit { ty: SubunitType::Monitor, id: 260 }, &[0b101, 0b11111111, 0b1]),
        ];

        let mut buf = BytesMut::new();
        for (unit, bytes) in testcases.iter() {
            buf.clear();
            buf.write_be(unit);
            assert_eq!(buf.chunk(), *bytes);
            let parsed: Subunit = buf.read_be().unwrap();
            assert_eq!(parsed, *unit);
        }
    }

    #[test]
    fn parse_frame() {
        let mut buf = Bytes::from_static(&[0x03, 0x48, 0x00]);
        let frame: Frame = buf.read_be().unwrap();
        assert_eq!(frame, Frame {
            ctype: CommandCodes::Notify,
            subunit: Subunit { ty: SubunitType::Panel, id: 0 },
            opcode: Opcode::VendorDependent,
        });
    }

}
