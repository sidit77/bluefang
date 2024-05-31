use std::fmt::{Display, Formatter};
use instructor::Instruct;

// ([Vol 3] Part B, Section 2.5.1).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Uuid(u128);

impl Uuid {
    const BASE: u128 = 0x00000000_0000_1000_8000_00805F9B34FB;

    #[inline]
    pub const fn from_u16(value: u16) -> Self {
        Self::from_u32(value as u32)
    }

    #[inline]
    pub const fn from_u32(value: u32) -> Self {
        Self(((value as u128) << 96) | Self::BASE)
    }

    #[inline]
    pub const fn from_u128(value: u128) -> Self {
        Self(value)
    }

    #[inline]
    fn remove_base(self) -> Option<u32> {
        ((self.0 & ((1u128 << 96) - 1)) == Self::BASE)
            .then_some((self.0 >> 96) as u32)
    }

    #[inline]
    pub fn as_packed(self) -> PackedUuid {
        match self.remove_base() {
            None => PackedUuid::Uuid128(self.0),
            Some(uuid32) => match u16::try_from(uuid32) {
                Ok(uuid16) => PackedUuid::Uuid16(uuid16),
                Err(_) => PackedUuid::Uuid32(uuid32)
            }
        }
    }

    #[inline]
    pub fn as_u16(self) -> Option<u16> {
        match self.as_packed() {
            PackedUuid::Uuid16(value) => Some(value),
            _ => None
        }
    }

}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Instruct)]
#[instructor(endian = "big")]
pub enum PackedUuid {
    Uuid16(u16),
    Uuid32(u32),
    Uuid128(u128)
}

impl PackedUuid {

    #[inline]
    pub const fn size_index(self) -> u8 {
        match self {
            Self::Uuid16(_) => 1,
            Self::Uuid32(_) => 2,
            Self::Uuid128(_) => 4
        }
    }

    #[inline]
    pub const fn byte_size(self) -> usize {
        1 << (self.size_index() as usize)
    }
}

impl From<u16> for Uuid {
    #[inline]
    fn from(value: u16) -> Self {
        Self::from_u16(value)
    }
}

impl From<u32> for Uuid {
    #[inline]
    fn from(value: u32) -> Self {
        Self::from_u32(value)
    }
}

impl From<u128> for Uuid {
    #[inline]
    fn from(value: u128) -> Self {
        Self::from_u128(value)
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

