use crate::hci::{Error, Hci, Opcode, OpcodeGroup};
use crate::hci::consts::{Lap, RemoteAddr, Role, Status};

impl Hci {

    /// Start the inquiry process to discover other Bluetooth devices in the vicinity.
    /// ([Vol 4] Part E, Section 7.1.1).
    ///
    /// # Parameters
    /// - `time`: The duration of the inquiry process in 1.28s units. Range: 1-30.
    /// - `max_responses`: The maximum number of responses to receive. 0 means no limit.
    pub async fn inquiry(&self, lap: Lap, time: u8, max_responses: u8) -> Result<(), Error> {
        self.call_with_args(Opcode::new(OpcodeGroup::LinkControl, 0x0001), |p| {
            p.u24(lap);
            p.u8(time);
            p.u8(max_responses);
        }).await?;
        // TODO return channel for inquiry results
        Ok(())
    }

    /// Accept a connection request from a remote device.
    /// ([Vol 4] Part E, Section 7.1.8).
    pub async fn accept_connection_request(&self, bd_addr: RemoteAddr, role: Role) -> Result<(), Error> {
        self.call_with_args(Opcode::new(OpcodeGroup::LinkControl, 0x0009), |p| {
            p.bytes(bd_addr.as_ref());
            p.u8(role);
        }).await?;
        Ok(())
    }

    pub async fn reject_connection_request(&self, bd_addr: RemoteAddr, reason: Status) -> Result<(), Error> {
        assert!(matches!(reason, Status::ConnectionRejectedDueToLimitedResources | Status::ConnectionRejectedDueToSecurityReasons | Status::ConnectionRejectedDueToUnacceptableBdAddr));
        self.call_with_args(Opcode::new(OpcodeGroup::LinkControl, 0x000A), |p| {
            p.bytes(bd_addr.as_ref());
            p.u8(reason);
        }).await?;
        Ok(())
    }
}

