use instructor::Exstruct;
use crate::hci::{Error, Hci};
use crate::hci::commands::{Opcode, OpcodeGroup};
use crate::hci::consts::{CompanyId, CoreVersion, RemoteAddr};

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

    /// Reads the maximum size of the data packets that the host can send to the controller
    /// ([Vol 4] Part E, Section 7.4.5).
    pub async fn read_buffer_size(&self) -> Result<BufferSizes, Error> {
        self.call(Opcode::new(OpcodeGroup::InfoParams, 0x0005)).await
    }

    /// ([Vol 4] Part E, Section 7.4.6).
    pub async fn read_bd_addr(&self) -> Result<RemoteAddr, Error> {
        self.call(Opcode::new(OpcodeGroup::InfoParams, 0x0009)).await
    }

}

/// `HCI_Read_Buffer_Size` return parameters
/// ([Vol 4] Part E, Section 7.4.5).
#[derive(Clone, Copy, Debug, Exstruct)]
pub struct BufferSizes {
    pub acl_data_packet_length: u16,
    pub synchronous_data_packet_length: u8,
    pub total_num_acl_data_packets: u16,
    pub total_num_synchronous_data_packets: u16,
}

/// `HCI_Read_Local_Supported_Commands` return parameter
/// ([Vol 4] Part E, Section 7.4.2).
#[derive(Clone, Copy, Debug, Exstruct)]
#[repr(transparent)]
pub struct SupportedCommands([u8; 64]);

impl Default for SupportedCommands {
    #[inline(always)]
    fn default() -> Self {
        Self([0; 64])
    }
}

/// `HCI_Read_Local_Version_Information` return parameters
/// ([Vol 4] Part E, Section 7.4.1).
#[derive(Clone, Copy, Debug, Default, Exstruct)]
pub struct LocalVersion {
    pub hci_version: CoreVersion,
    pub hci_subversion: u16,
    pub lmp_version: CoreVersion,
    pub company_id: CompanyId,
    pub lmp_subversion: u16,
}

