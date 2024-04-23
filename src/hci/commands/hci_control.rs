use crate::hci::{Error, Hci};
use crate::hci::commands::{Opcode, OpcodeGroup};

/// Controller and baseband commands ([Vol 4] Part E, Section 7.3).
impl Hci {
    /// Resets the controller's link manager, baseband, and link layer
    /// ([Vol 4] Part E, Section 7.3.2).
    pub async fn reset(&self) -> Result<(), Error> {
        self.call(Opcode::new(OpcodeGroup::HciControl, 0x0003)).await
    }
}