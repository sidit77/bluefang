use bitflags::bitflags;
use instructor::{Buffer, BufferMut, Exstruct, Instruct};
use tokio::sync::mpsc::unbounded_channel;
use crate::ensure;
use crate::hci::consts::{EventCode, RemoteAddr, Role, Status};
use crate::hci::{Error, Hci, Opcode, OpcodeGroup};

impl Hci {

    // ([Vol 4] Part E, Section 7.2.7).
    pub async fn discover_role(&self, handle: u16) -> Result<Role, Error> {
        let (_, role): (u16, Role) = self.call_with_args(Opcode::new(OpcodeGroup::LinkPolicy, 0x0009), |p| {
            p.write_le(handle);
        }).await?;
        Ok(role)
    }

    // ([Vol 4] Part E, Section 7.2.8).
    pub async fn switch_role(&self, addr: RemoteAddr, role: Role) -> Result<Role, Error> {
        let (tx, mut rx) = unbounded_channel();
        self.register_event_handler([EventCode::RoleChange], tx)?;
        self.call_with_args(Opcode::new(OpcodeGroup::LinkPolicy, 0x000B), |p| {
            p.write_le(addr);
            p.write_le(role);
        }).await?;
        while let Some((code, mut packet)) = rx.recv().await {
            assert_eq!(code, EventCode::RoleChange);
            let status: Status = packet.read_le()?;
            let target_addr: RemoteAddr = packet.read_le()?;
            let new_role: Role = packet.read_le()?;
            packet.finish()?;
            if target_addr == addr {
                ensure!(status.is_ok(), Error::Controller(status));
                return Ok(new_role);
            }
        }
        Err(Error::EventLoopClosed)
    }

    // ([Vol 4] Part E, Section 7.2.12).
    pub async fn set_default_link_policy_settings(&self, settings: LinkPolicy) -> Result<(), Error> {
        self.call_with_args(Opcode::new(OpcodeGroup::LinkPolicy, 0x000F), |p| {
            p.write_le(settings);
        }).await
    }

}

bitflags! {

    #[derive(Default, Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
    #[instructor(bitflags)]
    pub struct LinkPolicy: u16 {
        const ROLE_SWITCH = 0b001;
        const HOLD_MODE   = 0b010;
        const SNIFF_MODE  = 0b100;
    }
}