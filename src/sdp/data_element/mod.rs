mod uuid;

use instructor::{BigEndian, Buffer, BufferMut, Error as InstructorError, Exstruct, Instruct};
use instructor::utils::Limit;
use crate::ensure;

pub use uuid::Uuid;
use crate::sdp::error::Error;

// ([Vol 3] Part B, Section 3.2).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Exstruct)]
#[repr(u8)]
enum DataType {
    Nil = 0x00,
    UInt = 0x01,
    SInt = 0x02,
    Uuid = 0x03,
    Text = 0x04,
    Bool = 0x05,
    Sequence = 0x06,
    Alternative = 0x07,
    Url = 0x08,
}

impl DataType {

    fn valid_size_indices(self) -> &'static [u8] {
        match self {
            DataType::Nil => &[0],
            DataType::UInt => &[0, 1, 2, 3, 4],
            DataType::SInt => &[0, 1, 2, 3, 4],
            DataType::Uuid => &[1, 2, 4],
            DataType::Text => &[5, 6, 7],
            DataType::Bool => &[0],
            DataType::Sequence => &[5, 6, 7],
            DataType::Alternative => &[5, 6, 7],
            DataType::Url => &[5, 6, 7],
        }
    }

}

// ([Vol 3] Part B, Section 3.4).
#[derive(Debug, Exstruct)]
#[instructor(endian = "big")]
struct DataElementHeader {
    #[instructor(bitfield(u8))]
    #[instructor(bits(3..8))]
    data_type: DataType,
    #[instructor(bits(0..3))]
    size_index: u8
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
struct FullDataElementHeader {
    data_type: DataType,
    length: usize,
}

// ([Vol 3] Part B, Section 3.3).
impl Exstruct<BigEndian> for FullDataElementHeader {
    fn read_from_buffer<B: Buffer>(buffer: &mut B) -> Result<Self, InstructorError> {
        let DataElementHeader{ data_type, size_index } = buffer.read()?;
        ensure!(data_type.valid_size_indices().contains(&size_index), InstructorError::InvalidValue);
        let length = match size_index {
            0 if data_type == DataType::Nil => 0,
            0 => 1,
            1 => 2,
            2 => 4,
            3 => 8,
            4 => 16,
            5 => buffer.read_be::<u8>()? as usize,
            6 => buffer.read_be::<u16>()? as usize,
            7 => buffer.read_be::<u32>()? as usize,
            _ => return Err(InstructorError::InvalidValue)
        };
        ensure!(length <= buffer.remaining(), InstructorError::TooShort);
        Ok(Self {
            data_type,
            length,
        })
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum DataElement {
    Nil,
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
    Uuid(Uuid),
    Text(String),
    Bool(bool),
    Sequence(Vec<DataElement>),
    Alternative(Vec<DataElement>),
}

impl DataElement {

    pub fn as_sequence(&self) -> Result<&[DataElement], Error> {
        match self {
            DataElement::Sequence(sequence) => Ok(sequence),
            _ => Err(Error::UnexpectedDataType)
        }
    }

    pub fn as_uuid(&self) -> Result<Uuid, Error> {
        match self {
            DataElement::Uuid(uuid) => Ok(*uuid),
            _ => Err(Error::UnexpectedDataType)
        }
    }

    pub fn as_u32(&self) -> Result<u32, Error> {
        match self {
            DataElement::U32(value) => Ok(*value),
            _ => Err(Error::UnexpectedDataType)
        }
    }

    pub fn as_u16(&self) -> Result<u16, Error> {
        match self {
            DataElement::U16(value) => Ok(*value),
            _ => Err(Error::UnexpectedDataType)
        }
    }

}

impl Exstruct<BigEndian> for DataElement {
    fn read_from_buffer<B: Buffer>(buffer: &mut B) -> Result<Self, InstructorError> {
        let FullDataElementHeader { data_type, length } = buffer.read()?;

        match (data_type, length) {
            (DataType::Nil, 0) => Ok(Self::Nil),
            (DataType::UInt, 1) => Ok(Self::U8(buffer.read_be()?)),
            (DataType::UInt, 2) => Ok(Self::U16(buffer.read_be()?)),
            (DataType::UInt, 4) => Ok(Self::U32(buffer.read_be()?)),
            (DataType::UInt, 8) => Ok(Self::U64(buffer.read_be()?)),
            (DataType::UInt, 16) => Ok(Self::U128(buffer.read_be()?)),
            (DataType::SInt, 1) => Ok(Self::I8(buffer.read_be()?)),
            (DataType::SInt, 2) => Ok(Self::I16(buffer.read_be()?)),
            (DataType::SInt, 4) => Ok(Self::I32(buffer.read_be()?)),
            (DataType::SInt, 8) => Ok(Self::I64(buffer.read_be()?)),
            (DataType::SInt, 16) => Ok(Self::I128(buffer.read_be()?)),
            (DataType::Uuid, 2) => Ok(Self::Uuid(Uuid::from(buffer.read_be::<u16>()?))),
            (DataType::Uuid, 4) => Ok(Self::Uuid(Uuid::from(buffer.read_be::<u32>()?))),
            (DataType::Uuid, 16) => Ok(Self::Uuid(Uuid::from(buffer.read_be::<u128>()?))),
            (DataType::Text, n) => {
                let mut text = vec![0u8; n];
                buffer.try_copy_to_slice(&mut text)?;
                Ok(Self::Text(String::from_utf8(text).map_err(|_| InstructorError::InvalidValue)?))
            },
            (DataType::Bool, 1) => Ok(Self::Bool(buffer.read_be::<u8>()? != 0)),
            (DataType::Sequence, n) => {
                let mut buffer = Limit::new(buffer, n);
                let mut sequence = Vec::new();
                while buffer.remaining() > 0 {
                    sequence.push(buffer.read()?);
                }
                buffer.finish()?;
                Ok(Self::Sequence(sequence))
            },
            (DataType::Alternative, n) => {
                let mut buffer = Limit::new(buffer, n);
                let mut alternative = Vec::new();
                while buffer.remaining() > 0 {
                    alternative.push(buffer.read()?);
                }
                buffer.finish()?;
                Ok(Self::Alternative(alternative))
            },
            _ => Err(InstructorError::InvalidValue)
        }
    }
}

impl Instruct<BigEndian> for DataElement {
    fn write_to_buffer<B: BufferMut>(&self, buffer: &mut B) {

    }
}