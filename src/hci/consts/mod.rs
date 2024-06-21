mod class_of_device;
mod events;
mod company_id;

use std::fmt::{Debug, Display, Formatter};
use std::str::FromStr;
use std::time::Duration;
use instructor::utils::u24;
use instructor::{BufferMut, Endian, Exstruct, Instruct};

pub use class_of_device::*;
pub use events::*;
pub use company_id::CompanyId;


/// Bluetooth Core Specification versions ([Assigned Numbers] Section 2.1).
#[derive(Clone, Copy, Default, Eq, Ord, PartialEq, PartialOrd, Exstruct)]
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
    Unknown = 0xFF
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
            Self::Unknown => "<unknown version>"
        })
    }
}

/// LAPs ([Assigned Numbers] Section 2.2).
/// Range 0x9E8B00 to 0x9E8B3F
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u32)]
pub enum Lap {
    Limited = 0x9E8B00,
    General = 0x9E8B33
}

impl<E: Endian> Instruct<E> for Lap {
    fn write_to_buffer<B: BufferMut + ?Sized>(&self, buffer: &mut B) {
        buffer.write::<u24, E>((*self as u32).try_into().unwrap());
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Exstruct)]
#[repr(u8)]
pub enum LinkType {
    Sco = 0x00,
    Acl = 0x01,
    ESco = 0x02,
    #[instructor(default)]
    Unknown = 0xFF
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Exstruct, Instruct)]
#[repr(u8)]
pub enum Role {
    Master = 0x00,
    Slave = 0x01
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Exstruct, Instruct)]
pub struct RemoteAddr([u8; 6]);

impl Display for RemoteAddr {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            self.0[5], self.0[4], self.0[3], self.0[2], self.0[1], self.0[0]
        )
    }
}

impl FromStr for RemoteAddr {
    type Err = instructor::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut addr = [0; 6];
        let mut iter = s
            .split(':')
            .rev()
            .map(|s| u8::from_str_radix(s, 16)
                .map_err(|_| instructor::Error::InvalidValue));
        for i in addr.iter_mut() {
            *i = iter.next().ok_or(instructor::Error::TooShort)??;
        }
        Ok(Self(addr))
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for RemoteAddr {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for RemoteAddr {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where D: serde::Deserializer<'de> {
        struct RemoteAddrVisitor;

        impl serde::de::Visitor<'_> for RemoteAddrVisitor {
            type Value = RemoteAddr;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string in the format XX:XX:XX:XX:XX:XX")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> where E: serde::de::Error {
                value.parse().map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_str(RemoteAddrVisitor)
    }
}

impl From<[u8; 6]> for RemoteAddr {
    fn from(addr: [u8; 6]) -> Self {
        Self(addr)
    }
}

impl AsRef<[u8]> for RemoteAddr {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq, Exstruct, Instruct)]
pub struct LinkKey([u8; 16]);

impl Debug for LinkKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:02X?}", &self.0)
    }
}

impl Display for LinkKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for i in &self.0 {
            write!(f, "{:02X}", i)?;
        }
        Ok(())
    }
}

impl FromStr for LinkKey {
    type Err = instructor::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut key = [0; 16];
        let mut iter = (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16)
                .map_err(|_| instructor::Error::InvalidValue));
        for i in key.iter_mut() {
            *i = iter.next().ok_or(instructor::Error::TooShort)??;
        }
        Ok(Self(key))
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for LinkKey {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for LinkKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where D: serde::Deserializer<'de> {
        struct LinkKeyVisitor;

        impl serde::de::Visitor<'_> for LinkKeyVisitor {
            type Value = LinkKey;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a 16 byte hex string")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> where E: serde::de::Error {
                value.parse().map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_str(LinkKeyVisitor)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Exstruct, Instruct)]
#[repr(u8)]
pub enum LinkKeyType {
    Combination = 0x00,
    DebugCombination = 0x01,
    UnauthenticatedCombinationP192 = 0x02,
    AuthenticatedCombinationP192 = 0x03,
    ChangedCombination = 0x04,
    UnauthenticatedCombinationP256 = 0x05,
    AuthenticatedCombinationP256 = 0x06
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Exstruct, Instruct)]
#[repr(u8)]
pub enum IoCapability {
    DisplayOnly = 0x00,
    DisplayYesNo = 0x01,
    KeyboardOnly = 0x02,
    NoInputNoOutput = 0x03
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Exstruct, Instruct)]
#[repr(u8)]
pub enum AuthenticationRequirements {
    NoBondingUnprotected = 0x00,
    NoBondingProtected = 0x01,
    DedicatedBondingUnprotected = 0x02,
    DedicatedBondingProtected = 0x03,
    GeneralBondingUnprotected = 0x04,
    GeneralBondingProtected = 0x05
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Exstruct, Instruct)]
#[repr(u8)]
pub enum OobDataPresence {
    NotPresent = 0x00,
    P192Present = 0x01,
    P256Present = 0x02,
    P196AndP256Present = 0x03
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Exstruct, Instruct)]
#[repr(u8)]
pub enum KeypressNotificationType {
    EntryStarted = 0x00,
    DigitEntered = 0x01,
    DigitErased = 0x02,
    Cleared = 0x03,
    EntryCompleted = 0x04
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Exstruct, Instruct)]
#[repr(u8)]
pub enum ConnectionMode {
    #[default]
    Active = 0x00,
    Hold = 0x01,
    Sniff = 0x02,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Exstruct, Instruct)]
#[repr(u8)]
pub enum EncryptionMode {
    #[default]
    Off = 0x00,
    E0OrAesCcm = 0x01,
    AesCcm = 0x02,
}


pub const BASE_BAND_SLOT: Duration = Duration::from_nanos(625000);