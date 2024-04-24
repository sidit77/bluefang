use crate::hci::{Error, Hci};
use crate::hci::commands::{Opcode, OpcodeGroup};
use crate::hci::consts::ClassOfDevice;

/// Controller and baseband commands ([Vol 4] Part E, Section 7.3).
impl Hci {
    /// Resets the controller's link manager, baseband, and link layer
    /// ([Vol 4] Part E, Section 7.3.2).
    pub async fn reset(&self) -> Result<(), Error> {
        self.call(Opcode::new(OpcodeGroup::HciControl, 0x0003)).await
    }

    /// Sets the user-friendly name for the BR/EDR controller
    /// ([Vol 4] Part E, Section 7.3.11).
    pub async fn write_local_name(&self, name: &str) -> Result<(), Error> {
        assert!(name.len() < 248);
        self.call_with_args(Opcode::new(OpcodeGroup::HciControl, 0x0013), |p| {
            p.bytes(name.as_bytes());
            p.u8(0);
        }).await
    }

    /// Makes this device discoverable and/or connectable
    /// ([Vol 4] Part E, Section 7.3.18).
    pub async fn set_scan_enabled(&self, connectable: bool, discoverable: bool) -> Result<(), Error> {
        self.call_with_args(Opcode::new(OpcodeGroup::HciControl, 0x001A), |p| {
            p.u8(u8::from(connectable) << 1 | u8::from(discoverable));
        }).await
    }

    /// Sets the class of device
    /// ([Vol 4] Part E, Section 7.3.26).
    pub async fn write_class_of_device(&self, cod: ClassOfDevice) -> Result<(), Error> {
        self.call_with_args(Opcode::new(OpcodeGroup::HciControl, 0x0024), |p| {
            p.u24(cod);
        }).await
    }


}