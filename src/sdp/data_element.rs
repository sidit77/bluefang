use std::fmt::{Display, Formatter};
use std::marker::PhantomData;
use bytes::Bytes;
use instructor::{BigEndian, Buffer, Error, Exstruct};
use crate::ensure;

// ([Vol 3] Part B, Section 3.2).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Exstruct)]
#[repr(u8)]
pub enum DataType {
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
pub struct DataElement {
    pub data_type: DataType,
    pub length: usize,
}

// ([Vol 3] Part B, Section 3.3).
impl Exstruct<BigEndian> for DataElement {
    fn read_from_buffer<B: Buffer + ?Sized>(buffer: &mut B) -> Result<Self, Error> {
        let DataElementHeader{ data_type, size_index } = buffer.read()?;
        ensure!(data_type.valid_size_indices().contains(&size_index), Error::InvalidValue);
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
            _ => return Err(Error::InvalidValue)
        };
        ensure!(length <= buffer.remaining(), Error::TooShort);
        Ok(Self {
            data_type,
            length,
        })
    }
}

pub trait DataValue: Sized {

    fn try_read_as(data_element: DataElement, buffer: &mut Bytes) -> Option<Result<Self, Error>>;
}

impl DataValue for u32 {
    fn try_read_as(DataElement { data_type, length }: DataElement, buffer: &mut Bytes) -> Option<Result<Self, Error>> {
        ensure!(data_type == DataType::UInt && length == 4);
        Some(buffer.read_be::<Self>())
    }
}

pub struct Sequence<T> {
    data: Bytes,
    _phantom: PhantomData<T>
}

impl<T: DataValue> DataValue for Sequence<T> {
    fn try_read_as(DataElement { data_type, length }: DataElement, buffer: &mut Bytes) -> Option<Result<Self, Error>> {
        ensure!(data_type == DataType::Sequence);
        let data = buffer.split_to(length);
        Some(Ok(Self {
            data,
            _phantom: PhantomData
        }))
    }
}

impl<T: DataValue> Iterator for Sequence<T> {
    type Item = Result<T, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        (!self.data.is_empty())
            .then(|| self.data.read_data_element())
    }
}

// ([Vol 3] Part B, Section 2.5.1).
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Uuid(u128);

impl Uuid {
    const BASE: u128 = 0x00000000_0000_1000_8000_00805F9B34FB;
}

impl From<u16> for Uuid {
    fn from(value: u16) -> Self {
        Self(((value as u128) << 96) + Self::BASE)
    }
}

impl From<u32> for Uuid {
    fn from(value: u32) -> Self {
        Self(((value as u128) << 96) + Self::BASE)
    }
}

impl From<u128> for Uuid {
    fn from(value: u128) -> Self {
        Self(value)
    }
}

impl Display for Uuid {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:08X}-{:04X}-{:04X}-{:04X}-{:012X}",
            (self.0 >> 96) as u32,
            (self.0 >> 80) as u16,
            (self.0 >> 64) as u16,
            (self.0 >> 48) as u16,
            (self.0 & ((1 << 48) - 1)) as u64
        )
    }
}

impl DataValue for Uuid {
    fn try_read_as(DataElement { data_type, length }: DataElement, buffer: &mut Bytes) -> Option<Result<Self, Error>> {
        ensure!(data_type == DataType::Uuid);
        Some(match length {
            2 => buffer.read_be::<u16>().map(Self::from),
            4 => buffer.read_be::<u32>().map(Self::from),
            16 => buffer.read_be::<u128>().map(Self::from),
            _ => Err(Error::InvalidValue)
        })
    }
}

pub trait DataElementReader {
    fn read_data_element<T: DataValue>(&mut self) -> Result<T, Error>;
}

impl DataElementReader for Bytes {
    fn read_data_element<T: DataValue>(&mut self) -> Result<T, Error> {
        let data_element: DataElement = self.read()?;

        T::try_read_as(data_element, self)
            .ok_or(Error::InvalidValue)?
    }
}