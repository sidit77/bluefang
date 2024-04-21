
// Opcode group field definitions.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
#[repr(u16)]
pub enum OpcodeGroup {
    LinkControl = 0x01,
    LinkPolicy = 0x02,
    HciControl = 0x03,
    InfoParams = 0x04,
    StatusParams = 0x05,
    Testing = 0x06,
    Le = 0x08,
    Vendor = 0x3F, // [Vol 4] Part E, Section 5.4.1
}

#[derive(Default, Debug, Copy, Clone, Eq, PartialEq)]
pub struct Opcode(u16);

#[allow(dead_code)]
impl Opcode {

    /// Opcode 0x0000 is used to update `Num_HCI_Command_Packets`
    /// ([Vol 4] Part E, Section 7.7.14).
    const NONE: Opcode = Opcode(0x0000);

    // HCI Control and Baseband commands ([Vol 4] Part E, Section 7.3)
    pub const RESET: Opcode = Opcode::new(OpcodeGroup::HciControl, 0x0003);

}

impl Opcode {

    /// Creates a new opcode from the specified group and command fields.
    #[inline]
    pub const fn new(group: OpcodeGroup, ocf: u16) -> Self {
        // Combines OGF with OCF to create a full opcode.
        Self((group as u16) << 10 | ocf)
    }

}

impl From<Opcode> for u16 {
    #[inline]
    fn from(opcode: Opcode) -> u16 {
        opcode.0
    }
}