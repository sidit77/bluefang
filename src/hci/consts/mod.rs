
mod events;

use std::fmt::{Debug, Formatter};
use num_enum::{FromPrimitive, IntoPrimitive};
pub use events::*;

/// Bluetooth Core Specification versions ([Assigned Numbers] Section 2.1).
#[derive(Clone, Copy, Default, Eq, Ord, PartialEq, PartialOrd, FromPrimitive, IntoPrimitive)]
#[non_exhaustive]
#[repr(u8)]
pub enum CoreVersion {
    V1_0 = 0x00,
    V1_1 = 0x01,
    V1_2 = 0x02,
    V2_0 = 0x03,
    V2_1 = 0x04,
    V3_0 = 0x05,
    V4_0 = 0x06,
    V4_1 = 0x07,
    V4_2 = 0x08,
    V5_0 = 0x09,
    V5_1 = 0x0A,
    V5_2 = 0x0B,
    V5_3 = 0x0C,
    V5_4 = 0x0D,
    #[default]
    Unknown = 0xFF,
}

impl Debug for CoreVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match *self {
            Self::V1_0 => "v1.0b",
            Self::V1_1 => "v1.1",
            Self::V1_2 => "v1.2",
            Self::V2_0 => "v2.0+EDR",
            Self::V2_1 => "v2.1+EDR",
            Self::V3_0 => "v3.0+HS",
            Self::V4_0 => "v4.0",
            Self::V4_1 => "v4.1",
            Self::V4_2 => "v4.2",
            Self::V5_0 => "v5.0",
            Self::V5_1 => "v5.1",
            Self::V5_2 => "v5.2",
            Self::V5_3 => "v5.3",
            Self::V5_4 => "v5.4",
            Self::Unknown => "<unknown version>",
        })
    }
}

/// Company identifier ([Assigned Numbers] Section 7.1).
#[derive(Debug, Clone, Copy, Default, Eq, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct CompanyId(pub u16);