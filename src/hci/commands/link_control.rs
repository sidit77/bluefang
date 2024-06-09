use bytes::BufMut;
use instructor::BufferMut;

use crate::hci::consts::{AuthenticationRequirements, IoCapability, Lap, LinkKey, OobDataPresence, RemoteAddr, Role, Status};
use crate::hci::{Error, Hci, Opcode, OpcodeGroup};

impl Hci {
    /// Start the inquiry process to discover other Bluetooth devices in the vicinity.
    /// ([Vol 4] Part E, Section 7.1.1).
    ///
    /// # Parameters
    /// - `time`: The duration of the inquiry process in 1.28s units. Range: 1-30.
    /// - `max_responses`: The maximum number of responses to receive. 0 means no limit.
    pub async fn inquiry(&self, lap: Lap, time: u8, max_responses: u8) -> Result<(), Error> {
        self.call_with_args(Opcode::new(OpcodeGroup::LinkControl, 0x0001), |p| {
            p.write_le(lap);
            p.write_le(time);
            p.write_le(max_responses);
        })
        .await?;
        // TODO return channel for inquiry results
        Ok(())
    }

    /// Accept a connection request from a remote device.
    /// ([Vol 4] Part E, Section 7.1.8).
    pub async fn accept_connection_request(&self, bd_addr: RemoteAddr, role: Role) -> Result<(), Error> {
        self.call_with_args(Opcode::new(OpcodeGroup::LinkControl, 0x0009), |p| {
            p.write_le(bd_addr);
            p.write_le(role);
        })
        .await?;
        Ok(())
    }

    /// Reject a connection request from a remote device.
    /// ([Vol 4] Part E, Section 7.1.9).
    pub async fn reject_connection_request(&self, bd_addr: RemoteAddr, reason: Status) -> Result<(), Error> {
        assert!(matches!(
            reason,
            Status::ConnectionRejectedDueToLimitedResources
                | Status::ConnectionRejectedDueToSecurityReasons
                | Status::ConnectionRejectedDueToUnacceptableBdAddr
        ));
        self.call_with_args(Opcode::new(OpcodeGroup::LinkControl, 0x000A), |p| {
            p.write_le(bd_addr);
            p.write_le(reason);
        })
        .await?;
        Ok(())
    }

    /// ([Vol 4] Part E, Section 7.1.10).
    pub async fn link_key_present(&self, bd_addr: RemoteAddr, key: &LinkKey) -> Result<RemoteAddr, Error> {
        self.call_with_args(Opcode::new(OpcodeGroup::LinkControl, 0x000B), |p| {
            p.write_le(bd_addr);
            p.write_le_ref(key);
        })
        .await
    }

    /// ([Vol 4] Part E, Section 7.1.11).
    pub async fn link_key_not_present(&self, bd_addr: RemoteAddr) -> Result<RemoteAddr, Error> {
        self.call_with_args(Opcode::new(OpcodeGroup::LinkControl, 0x000C), |p| {
            p.write_le(bd_addr);
        })
        .await
    }

    /// ([Vol 4] Part E, Section 7.1.12).
    pub async fn pin_code_request_reply(&self, bd_addr: RemoteAddr, pin: &str) -> Result<RemoteAddr, Error> {
        assert!(pin.len() <= 16);
        self.call_with_args(Opcode::new(OpcodeGroup::LinkControl, 0x000D), |p| {
            p.write_le(bd_addr);
            p.write_le(pin.len());
            p.put_slice(pin.as_bytes());
            p.put_bytes(0, 16 - pin.len());
        })
        .await
    }

    /// ([Vol 4] Part E, Section 7.1.29).
    pub async fn io_capability_reply(
        &self, bd_addr: RemoteAddr, io: IoCapability, oob: OobDataPresence, auth: AuthenticationRequirements
    ) -> Result<RemoteAddr, Error> {
        self.call_with_args(Opcode::new(OpcodeGroup::LinkControl, 0x002B), |p| {
            p.write_le(bd_addr);
            p.write_le(io);
            p.write_le(oob);
            p.write_le(auth);
        })
        .await
    }

    /// ([Vol 4] Part E, Section 7.1.30).
    pub async fn user_confirmation_request_accept(&self, bd_addr: RemoteAddr) -> Result<RemoteAddr, Error> {
        self.call_with_args(Opcode::new(OpcodeGroup::LinkControl, 0x002C), |p| {
            p.write_le(bd_addr);
        })
        .await
    }
}
