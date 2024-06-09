use crate::hci::consts::CoreVersion;
use crate::hci::consts::CoreVersion::{V1_0, V4_0, V4_1, V4_2, V5_1, V5_2, V5_3};

pub const RTL_ROM_LMP_8703B: u16 = 0x8703;
pub const RTL_ROM_LMP_8723A: u16 = 0x1200;
pub const RTL_ROM_LMP_8723B: u16 = 0x8723;
pub const RTL_ROM_LMP_8821A: u16 = 0x8821;
pub const RTL_ROM_LMP_8761A: u16 = 0x8761;
pub const RTL_ROM_LMP_8822B: u16 = 0x8822;
pub const RTL_ROM_LMP_8852A: u16 = 0x8852;
pub const RTL_ROM_LMP_8851B: u16 = 0x8851;

//const CHIP_ID_8723A: u16 = 0x00;
//const CHIP_ID_8723B: u16 = 0x01;
//const CHIP_ID_8821A: u16 = 0x02;
//const CHIP_ID_8761A: u16 = 0x03;
//const CHIP_ID_8822B: u16 = 0x08;
//const CHIP_ID_8723D: u16 = 0x09;
//const CHIP_ID_8821C: u16 = 0x0A;
//const CHIP_ID_8822C: u16 = 0x0D;
//const CHIP_ID_8761B: u16 = 0x0E;
//const CHIP_ID_8852A: u16 = 0x12;
//const CHIP_ID_8852B: u16 = 0x14;
//const CHIP_ID_8852C: u16 = 0x19;
//const CHIP_ID_8851B: u16 = 0x24;
//const CHIP_ID_8852BT: u16 = 0x2F;

const CHIP_TYPE_8723CS_CG: u8 = 0x03;
const CHIP_TYPE_8723CS_VF: u8 = 0x04;
const CHIP_TYPE_8723CS_XX: u8 = 0x05;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum HciBus {
    Uart,
    Usb
}

struct MatchFlags;

impl MatchFlags {
    const LMP_SUBVERSION: u8 = 0x01;
    const HCI_SUBVERSION: u8 = 0x02;
    const HCI_VERSION: u8 = 0x04;
    const HCI_BUS: u8 = 0x08;
    const HCI_CHIP_ID: u8 = 0x10;
}

#[allow(dead_code)]
#[derive(Debug, Copy, Clone)]
pub struct DriverInfo {
    flags: u8,
    pub lmp_subversion: u16,
    pub hci_subversion: u16,
    pub hci_version: CoreVersion,
    pub hci_bus: HciBus,
    pub chip_type: u8,
    pub config_needed: bool,
    pub has_rom_version: bool,
    pub has_msft_ext: bool,
    pub firmware_name: &'static str,
    pub config_name: &'static str,
    pub chip_name: &'static str
}

impl DriverInfo {
    const fn is_enabled(&self, flags: u8) -> bool {
        self.flags & flags == flags
    }

    pub fn matches(&self, lmp_subversion: u16, hci_subversion: u16, hci_version: CoreVersion, hci_bus: HciBus, chip_type: u8) -> bool {
        (!self.is_enabled(MatchFlags::LMP_SUBVERSION) || self.lmp_subversion == lmp_subversion)
            && (!self.is_enabled(MatchFlags::HCI_SUBVERSION) || self.hci_subversion == hci_subversion)
            && (!self.is_enabled(MatchFlags::HCI_VERSION) || self.hci_version == hci_version)
            && (!self.is_enabled(MatchFlags::HCI_BUS) || self.hci_bus == hci_bus)
            && (!self.is_enabled(MatchFlags::HCI_CHIP_ID) || self.chip_type == chip_type)
    }
}

pub const DRIVER_INFOS: &[DriverInfo] = &[
    /* 8723A */
    DriverInfo {
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS,
        lmp_subversion: RTL_ROM_LMP_8723A,
        hci_subversion: 0xb,
        hci_version: V4_0,
        hci_bus: HciBus::Usb,
        chip_type: 0,
        config_needed: false,
        has_rom_version: false,
        has_msft_ext: false,
        firmware_name: "rtl8723a_fw.bin",
        config_name: "",
        chip_name: "rtl8723au"
    },
    /* 8723BS */
    DriverInfo {
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS,
        lmp_subversion: RTL_ROM_LMP_8723B,
        hci_subversion: 0xb,
        hci_version: V4_0,
        hci_bus: HciBus::Uart,
        chip_type: 0,
        config_needed: true,
        has_rom_version: true,
        has_msft_ext: false,
        firmware_name: "rtl8723bs_fw.bin",
        config_name: "rtl8723bs_config.bin",
        chip_name: "rtl8723bs"
    },
    /* 8723B */
    DriverInfo {
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS,
        lmp_subversion: RTL_ROM_LMP_8723B,
        hci_subversion: 0xb,
        hci_version: V4_0,
        hci_bus: HciBus::Usb,
        chip_type: 0,
        config_needed: false,
        has_rom_version: true,
        has_msft_ext: false,
        firmware_name: "rtl8723b_fw.bin",
        config_name: "rtl8723b_config.bin",
        chip_name: "rtl8723bu"
    },
    /* 8723CS-CG */
    DriverInfo {
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_CHIP_ID | MatchFlags::HCI_BUS,
        lmp_subversion: RTL_ROM_LMP_8703B,
        hci_subversion: 0,
        chip_type: CHIP_TYPE_8723CS_CG,
        hci_bus: HciBus::Uart,
        config_needed: true,
        has_rom_version: true,
        has_msft_ext: false,
        firmware_name: "rtl8723cs_cg_fw.bin",
        config_name: "rtl8723cs_cg_config.bin",
        chip_name: "rtl8723cs-cg",
        hci_version: V1_0
    },
    /* 8723CS-VF */
    DriverInfo {
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_CHIP_ID | MatchFlags::HCI_BUS,
        lmp_subversion: RTL_ROM_LMP_8703B,
        hci_subversion: 0,
        chip_type: CHIP_TYPE_8723CS_VF,
        hci_bus: HciBus::Uart,
        config_needed: true,
        has_rom_version: true,
        has_msft_ext: false,
        firmware_name: "rtl8723cs_vf_fw.bin",
        config_name: "rtl8723cs_vf_config.bin",
        chip_name: "rtl8723cs-vf",
        hci_version: V1_0
    },
    /* 8723CS-XX */
    DriverInfo {
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_CHIP_ID | MatchFlags::HCI_BUS,
        lmp_subversion: RTL_ROM_LMP_8703B,
        hci_subversion: 0,
        chip_type: CHIP_TYPE_8723CS_XX,
        hci_bus: HciBus::Uart,
        config_needed: true,
        has_rom_version: true,
        has_msft_ext: false,
        firmware_name: "rtl8723cs_xx_fw.bin",
        config_name: "rtl8723cs_xx_config.bin",
        chip_name: "rtl8723cs",
        hci_version: V1_0
    },
    /* 8723D */
    DriverInfo {
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS,
        lmp_subversion: RTL_ROM_LMP_8723B,
        hci_subversion: 0xd,
        hci_version: V4_2,
        hci_bus: HciBus::Usb,
        chip_type: 0,
        config_needed: true,
        has_rom_version: true,
        has_msft_ext: false,
        firmware_name: "rtl8723d_fw.bin",
        config_name: "rtl8723d_config.bin",
        chip_name: "rtl8723du"
    },
    /* 8723DS */
    DriverInfo {
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS,
        lmp_subversion: RTL_ROM_LMP_8723B,
        hci_subversion: 0xd,
        hci_version: V4_2,
        hci_bus: HciBus::Uart,
        chip_type: 0,
        config_needed: true,
        has_rom_version: true,
        has_msft_ext: false,
        firmware_name: "rtl8723ds_fw.bin",
        config_name: "rtl8723ds_config.bin",
        chip_name: "rtl8723ds"
    },
    /* 8821A */
    DriverInfo {
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS,
        lmp_subversion: RTL_ROM_LMP_8821A,
        hci_subversion: 0xa,
        hci_version: V4_0,
        hci_bus: HciBus::Usb,
        chip_type: 0,
        config_needed: false,
        has_rom_version: true,
        has_msft_ext: false,
        firmware_name: "rtl8821a_fw.bin",
        config_name: "rtl8821a_config.bin",
        chip_name: "rtl8821au"
    },
    /* 8821C */
    DriverInfo {
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS,
        lmp_subversion: RTL_ROM_LMP_8821A,
        hci_subversion: 0xc,
        hci_version: V4_2,
        hci_bus: HciBus::Usb,
        chip_type: 0,
        config_needed: false,
        has_rom_version: true,
        has_msft_ext: true,
        firmware_name: "rtl8821c_fw.bin",
        config_name: "rtl8821c_config.bin",
        chip_name: "rtl8821cu"
    },
    /* 8821CS */
    DriverInfo {
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS,
        lmp_subversion: RTL_ROM_LMP_8821A,
        hci_subversion: 0xc,
        hci_version: V4_2,
        hci_bus: HciBus::Uart,
        chip_type: 0,
        config_needed: true,
        has_rom_version: true,
        has_msft_ext: true,
        firmware_name: "rtl8821cs_fw.bin",
        config_name: "rtl8821cs_config.bin",
        chip_name: "rtl8821cs"
    },
    /* 8761A */
    DriverInfo {
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS,
        lmp_subversion: RTL_ROM_LMP_8761A,
        hci_subversion: 0xa,
        hci_version: V4_0,
        hci_bus: HciBus::Usb,
        chip_type: 0,
        config_needed: false,
        has_rom_version: true,
        has_msft_ext: false,
        firmware_name: "rtl8761a_fw.bin",
        config_name: "rtl8761a_config.bin",
        chip_name: "rtl8761au"
    },
    /* 8761B */
    DriverInfo {
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS,
        lmp_subversion: RTL_ROM_LMP_8761A,
        hci_subversion: 0xb,
        hci_version: V5_1,
        hci_bus: HciBus::Uart,
        chip_type: 0,
        config_needed: false,
        has_rom_version: true,
        has_msft_ext: true,
        firmware_name: "rtl8761b_fw.bin",
        config_name: "rtl8761b_config.bin",
        chip_name: "rtl8761btv"
    },
    /* 8761BU */
    DriverInfo {
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS,
        lmp_subversion: RTL_ROM_LMP_8761A,
        hci_subversion: 0xb,
        hci_version: V5_1,
        hci_bus: HciBus::Usb,
        chip_type: 0,
        config_needed: false,
        has_rom_version: true,
        has_msft_ext: false,
        firmware_name: "rtl8761bu_fw.bin",
        config_name: "rtl8761bu_config.bin",
        chip_name: "rtl8761bu"
    },
    /* 8822C with UART interface */
    DriverInfo {
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS,
        lmp_subversion: RTL_ROM_LMP_8822B,
        hci_subversion: 0xc,
        hci_version: V4_2,
        hci_bus: HciBus::Uart,
        chip_type: 0,
        config_needed: true,
        has_rom_version: true,
        has_msft_ext: true,
        firmware_name: "rtl8822cs_fw.bin",
        config_name: "rtl8822cs_config.bin",
        chip_name: "rtl8822cs"
    },
    /* 8822C with UART interface */
    DriverInfo {
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS,
        lmp_subversion: RTL_ROM_LMP_8822B,
        hci_subversion: 0xc,
        hci_version: V5_1,
        hci_bus: HciBus::Uart,
        chip_type: 0,
        config_needed: true,
        has_rom_version: true,
        has_msft_ext: true,
        firmware_name: "rtl8822cs_fw.bin",
        config_name: "rtl8822cs_config.bin",
        chip_name: "rtl8822cs"
    },
    /* 8822C with USB interface */
    DriverInfo {
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS,
        lmp_subversion: RTL_ROM_LMP_8822B,
        hci_subversion: 0xc,
        hci_version: V5_1,
        hci_bus: HciBus::Usb,
        chip_type: 0,
        config_needed: false,
        has_rom_version: true,
        has_msft_ext: true,
        firmware_name: "rtl8822cu_fw.bin",
        config_name: "rtl8822cu_config.bin",
        chip_name: "rtl8822cu"
    },
    /* 8822B */
    DriverInfo {
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS,
        lmp_subversion: RTL_ROM_LMP_8822B,
        hci_subversion: 0xb,
        hci_version: V4_1,
        hci_bus: HciBus::Usb,
        chip_type: 0,
        config_needed: true,
        has_rom_version: true,
        has_msft_ext: true,
        firmware_name: "rtl8822b_fw.bin",
        config_name: "rtl8822b_config.bin",
        chip_name: "rtl8822bu"
    },
    /* 8852A */
    DriverInfo {
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS,
        lmp_subversion: RTL_ROM_LMP_8852A,
        hci_subversion: 0xa,
        hci_version: V5_2,
        hci_bus: HciBus::Usb,
        chip_type: 0,
        config_needed: false,
        has_rom_version: true,
        has_msft_ext: true,
        firmware_name: "rtl8852au_fw.bin",
        config_name: "rtl8852au_config.bin",
        chip_name: "rtl8852au"
    },
    /* 8852B with UART interface */
    DriverInfo {
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS,
        lmp_subversion: RTL_ROM_LMP_8852A,
        hci_subversion: 0xb,
        hci_version: V5_2,
        hci_bus: HciBus::Uart,
        chip_type: 0,
        config_needed: true,
        has_rom_version: true,
        has_msft_ext: true,
        firmware_name: "rtl8852bs_fw.bin",
        config_name: "rtl8852bs_config.bin",
        chip_name: "rtl8852bs"
    },
    /* 8852B */
    DriverInfo {
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS,
        lmp_subversion: RTL_ROM_LMP_8852A,
        hci_subversion: 0xb,
        hci_version: V5_2,
        hci_bus: HciBus::Usb,
        chip_type: 0,
        config_needed: false,
        has_rom_version: true,
        has_msft_ext: true,
        firmware_name: "rtl8852bu_fw.bin",
        config_name: "rtl8852bu_config.bin",
        chip_name: "rtl8852bu"
    },
    /* 8852C */
    DriverInfo {
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS,
        lmp_subversion: RTL_ROM_LMP_8852A,
        hci_subversion: 0xc,
        hci_version: V5_3,
        hci_bus: HciBus::Usb,
        chip_type: 0,
        config_needed: false,
        has_rom_version: true,
        has_msft_ext: true,
        firmware_name: "rtl8852cu_fw.bin",
        config_name: "rtl8852cu_config.bin",
        chip_name: "rtl8852cu"
    },
    /* 8851B */
    DriverInfo {
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS,
        lmp_subversion: RTL_ROM_LMP_8851B,
        hci_subversion: 0xb,
        hci_version: V5_3,
        hci_bus: HciBus::Usb,
        chip_type: 0,
        config_needed: false,
        has_rom_version: true,
        has_msft_ext: false,
        firmware_name: "rtl8851bu_fw.bin",
        config_name: "rtl8851bu_config.bin",
        chip_name: "rtl8851bu"
    },
    /* 8852BT/8852BE-VT */
    DriverInfo {
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS,
        lmp_subversion: RTL_ROM_LMP_8852A,
        hci_subversion: 0x87,
        hci_version: V5_3,
        hci_bus: HciBus::Usb,
        chip_type: 0,
        config_needed: false,
        has_rom_version: true,
        has_msft_ext: true,
        firmware_name: "rtl8852btu_fw.bin",
        config_name: "rtl8852btu_config.bin",
        chip_name: "rtl8852btu"
    }
];
