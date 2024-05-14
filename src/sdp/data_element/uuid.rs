use std::fmt::{Display, Formatter};

// ([Vol 3] Part B, Section 2.5.1).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Uuid(u128);

impl Uuid {
    const BASE: u128 = 0x00000000_0000_1000_8000_00805F9B34FB;

    pub const fn from_u16(value: u16) -> Self {
        Self(((value as u128) << 96) + Self::BASE)
    }

    pub const fn from_u32(value: u32) -> Self {
        Self(((value as u128) << 96) + Self::BASE)
    }

    pub const fn from_u128(value: u128) -> Self {
        Self(value)
    }
}

impl From<u16> for Uuid {
    fn from(value: u16) -> Self {
        Self::from_u16(value)
    }
}

impl From<u32> for Uuid {
    fn from(value: u32) -> Self {
        Self::from_u32(value)
    }
}

impl From<u128> for Uuid {
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

