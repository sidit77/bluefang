mod hci_control;
mod info_params;
mod link_control;
mod link_policy;

use std::fmt::{Debug, Formatter};
use instructor::Exstruct;
use num_enum::TryFromPrimitive;

pub use info_params::*;
pub use link_control::*;

//pub use hci_control::*;

// Opcode group field definitions.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Eq, PartialEq, TryFromPrimitive)]
#[repr(u16)]
pub enum OpcodeGroup {
    LinkControl = 0x01,
    LinkPolicy = 0x02,
    HciControl = 0x03,
    InfoParams = 0x04,
    StatusParams = 0x05,
    Testing = 0x06,
    Le = 0x08,
    Vendor = 0x3F // [Vol 4] Part E, Section 5.4.1
}

#[derive(Default, Copy, Clone, Eq, PartialEq, Exstruct)]
pub struct Opcode(u16);

#[allow(dead_code)]
impl Opcode {
    /// Opcode 0x0000 is used to update `Num_HCI_Command_Packets`
    /// ([Vol 4] Part E, Section 7.7.14).
    const NONE: Opcode = Opcode(0x0000);
}

impl Opcode {
    /// Creates a new opcode from the specified group and command fields.
    #[inline]
    pub const fn new(group: OpcodeGroup, ocf: u16) -> Self {
        // Combines OGF with OCF to create a full opcode.
        Self((group as u16) << 10 | ocf)
    }

    pub fn split(&self) -> Option<(OpcodeGroup, u16)> {
        OpcodeGroup::try_from((self.0 >> 10) & 0x3F)
            .ok()
            .map(|group| (group, self.0 & 0x3FF))
    }
}

impl Debug for Opcode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.split() {
            Some((group, ocf)) => write!(f, "Opcode({:?}, 0x{:03X})", group, ocf),
            None => write!(f, "Opcode(0x{:04X})", self.0)
        }
    }
}

impl From<Opcode> for u16 {
    #[inline]
    fn from(opcode: Opcode) -> u16 {
        opcode.0
    }
}

impl From<u16> for Opcode {
    #[inline]
    fn from(opcode: u16) -> Opcode {
        Opcode(opcode)
    }
}
