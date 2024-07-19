use instructor::BufferMut;
use crate::hci::consts::{RemoteAddr, Role};
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
    pub async fn switch_role(&self, addr: RemoteAddr, role: Role) -> Result<(), Error> {
        self.call_with_args(Opcode::new(OpcodeGroup::LinkPolicy, 0x000B), |p| {
            p.write_le(addr);
            p.write_le(role);
        }).await?;
        Ok(())
    }

}