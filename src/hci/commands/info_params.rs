use crate::hci::{Error, Hci};
use crate::hci::buffer::ReceiveBuffer;
use crate::hci::commands::{Opcode, OpcodeGroup};
use crate::hci::consts::{CompanyId, CoreVersion};
use crate::hci::events::FromEvent;

/// Informational parameters commands ([Vol 4] Part E, Section 7.4).
impl Hci {

    /// Returns the controller's version information
    /// ([Vol 4] Part E, Section 7.4.1).
    pub async fn read_local_version(&self) -> Result<LocalVersion, Error> {
        self.call(Opcode::new(OpcodeGroup::InfoParams, 0x0001)).await
    }

    /// Returns the controller's supported commands
    /// ([Vol 4] Part E, Section 7.4.2).
    pub async fn read_local_supported_commands(&self) -> Result<SupportedCommands, Error> {
        self.call(Opcode::new(OpcodeGroup::InfoParams, 0x0002)).await
    }

}

/// `HCI_Read_Local_Supported_Commands` return parameter
/// ([Vol 4] Part E, Section 7.4.2).
#[derive(Clone, Copy, Debug)]
#[repr(transparent)]
pub struct SupportedCommands([u8; 64]);

impl Default for SupportedCommands {
    #[inline(always)]
    fn default() -> Self {
        Self([0; 64])
    }
}

impl FromEvent for SupportedCommands {

    #[inline]
    fn unpack(buf: &mut ReceiveBuffer) -> Option<Self> {
        buf.get_bytes().map(Self)
    }
}

/// `HCI_Read_Local_Version_Information` return parameters
/// ([Vol 4] Part E, Section 7.4.1).
#[derive(Clone, Copy, Debug, Default)]
pub struct LocalVersion {
    pub hci_version: CoreVersion,
    pub hci_subversion: u16,
    pub lmp_version: CoreVersion,
    pub company_id: CompanyId,
    pub lmp_subversion: u16,
}

impl FromEvent for LocalVersion {
    #[inline]
    fn unpack(buf: &mut ReceiveBuffer) -> Option<Self> {
        Some(Self {
            hci_version: CoreVersion::from(buf.get_u8()?),
            hci_subversion: buf.get_u16()?,
            lmp_version: CoreVersion::from(buf.get_u8()?),
            company_id: CompanyId(buf.get_u16()?),
            lmp_subversion: buf.get_u16()?,
        })
    }
}
