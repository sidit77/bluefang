use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use bytes::BufMut;
use tracing::{debug, error, trace};
use crate::ensure;
use crate::hci::{Error, FirmwareLoader, Hci, LocalVersion, Opcode, OpcodeGroup};
use crate::hci::consts::CoreVersion;
use crate::hci::consts::CoreVersion::*;

const RTL_ROM_LMP_8703B: u16 = 0x8703;
const RTL_ROM_LMP_8723A: u16 = 0x1200;
const RTL_ROM_LMP_8723B: u16 = 0x8723;
const RTL_ROM_LMP_8821A: u16 = 0x8821;
const RTL_ROM_LMP_8761A: u16 = 0x8761;
const RTL_ROM_LMP_8822B: u16 = 0x8822;
const RTL_ROM_LMP_8852A: u16 = 0x8852;
const RTL_ROM_LMP_8851B: u16 = 0x8851;

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
enum HciBus {
    Uart, Usb
}

struct MatchFlags;

impl MatchFlags {
    const LMP_SUBVERSION: u8 = 0x01;
    const HCI_SUBVERSION: u8 = 0x02;
    const HCI_VERSION: u8 = 0x04;
    const HCI_BUS: u8 = 0x08;
    const HCI_CHIP_ID: u8 = 0x10;
}

//bitflags! {
//    #[derive(Debug, Copy, Clone, Eq, PartialEq)]
//    struct MatchFlags: u8 {
//        const LMP_SUBVERSION = 0x01;
//        const HCI_SUBVERSION = 0x02;
//        const HCI_VERSION = 0x04;
//        const HCI_BUS = 0x08;
//        const HCI_CHIP_ID = 0x10;
//    }
//}

#[allow(dead_code)]
#[derive(Debug, Copy, Clone)]
struct DriverInfo {
    flags: u8,
    lmp_subversion: u16,
    hci_subversion: u16,
    hci_version: CoreVersion,
    hci_bus: HciBus,
    chip_type: u8,
    config_needed: bool,
    has_rom_version: bool,
    has_msft_ext: bool,
    firmware_name: &'static str,
    config_name: &'static str,
    chip_name: &'static str,
}

impl DriverInfo {

    const fn is_enabled(&self, flags: u8) -> bool {
        self.flags & flags == flags
    }

    fn matches(&self, lmp_subversion: u16, hci_subversion: u16, hci_version: CoreVersion, hci_bus: HciBus, chip_type: u8) -> bool {
        (!self.is_enabled(MatchFlags::LMP_SUBVERSION) || self.lmp_subversion == lmp_subversion) &&
        (!self.is_enabled(MatchFlags::HCI_SUBVERSION) || self.hci_subversion == hci_subversion) &&
        (!self.is_enabled(MatchFlags::HCI_VERSION) || self.hci_version == hci_version) &&
        (!self.is_enabled(MatchFlags::HCI_BUS) || self.hci_bus == hci_bus) &&
        (!self.is_enabled(MatchFlags::HCI_CHIP_ID) || self.chip_type == chip_type)
    }

}

const DRIVER_INFOS: &[DriverInfo] = &[

    /* 8723A */
    DriverInfo {
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS ,
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
        chip_name: "rtl8723au",
    },

    /* 8723BS */
    DriverInfo {
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS ,
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
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS ,
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
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_CHIP_ID |MatchFlags::HCI_BUS,
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
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS ,
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
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS ,
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
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS ,
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
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS ,
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
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS ,
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
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS ,
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
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS ,
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
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS ,
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
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS ,
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
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS ,
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
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS ,
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
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS ,
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
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS ,
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
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS ,
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
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS ,
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
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS ,
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
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS ,
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
        flags: MatchFlags::LMP_SUBVERSION | MatchFlags::HCI_SUBVERSION | MatchFlags::HCI_VERSION | MatchFlags::HCI_BUS ,
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
    },

];

pub fn find_binary_path(file_name: &str) -> Option<PathBuf> {
    //TODO automatically download the firmware
    let dir: PathBuf = std::env::var_os("RTK_FIRMWARE_DIR")?.into();

    let file = dir.join(file_name);
    file.exists().then_some(file)
}

const RTK_FRAGMENT_LENGTH: usize = 252;

async fn download_for_rtl8723b(host: &Hci, info: DriverInfo, firmware: Vec<u8>, config: Option<Vec<u8>>) -> Result<(), Error> {
    let version = if info.has_rom_version {
        let version= host.read_rom_version().await?;
        debug!("firmware version before download: {}", version);
        version as u16
    } else {
        0
    };
    let firmware = Firmware::from_bytes(&firmware)?;
    debug!("firmware: project_id=0x{:04X}", firmware.project_id);
    let patch = firmware
        .patches
        .into_iter()
        .find(|patch| patch.chip_id == version + 1)
        .ok_or(Error::from("Failed to find patch for current firmware version"))?;
    debug!("using patch {}", patch.chip_id);
    let mut payload = patch.data;
    // Append the config if there is one.
    if let Some(config) = config {
        payload.extend(config);
    }

    // Download the payload, one fragment at a time.
    for (fragment_index, fragment) in payload.chunks(RTK_FRAGMENT_LENGTH).into_iter().enumerate() {
        // NOTE: the Linux driver somehow adds 1 to the index after it wraps around.
        // That's odd, but we"ll do the same here.
        let mut download_index = fragment_index & 0x7F;
        if download_index >= 0x80 {
            download_index += 1;
        }
        if (fragment_index + 1) * RTK_FRAGMENT_LENGTH >= payload.len() {
            download_index |= 0x80; // End marker
        }
        debug!("downloading fragment {}", fragment_index);
        host.download(download_index as u8, fragment).await?;
    }
    debug!("download complete");
    host.read_rom_version()
        .await
        .map(|v| debug!("firmware version after download: {}", v))
        .unwrap_or_else(|err| error!("failed to read firmware version: {}", err));


    Ok(())
}

const RTL_CHIP_SUBVER: [u8; 5] = [0x10, 0x38, 0x04, 0x28, 0x80];
const RTL_CHIP_REV   : [u8; 5] = [0x10, 0x3A, 0x04, 0x28, 0x80];
const RTL_SEC_PROJ   : [u8; 5] = [0x10, 0xA4, 0x0D, 0x00, 0xb0];
const RTL_CPIP_TYPE   : [u8; 5] = [0x00, 0x94, 0xa0, 0x00, 0xb0];

#[derive(Default, Debug, Copy, Clone)]
pub struct RealTekFirmwareLoader;

impl RealTekFirmwareLoader {
    pub fn new() -> Self {
        Self::default()
    }

    async fn find_chip_info(&self, hci: &Hci) -> Result<(u16, u16, CoreVersion, u8), Error> {
        let lmp_subversion = hci.read_reg16(RTL_CHIP_SUBVER).await?;
        if lmp_subversion == RTL_ROM_LMP_8822B {
            let hci_subversion = hci.read_reg16(RTL_CHIP_REV).await?;
            if hci_subversion == 0x0003 {
                return Ok((lmp_subversion, hci_subversion, V5_3, 0));
            }
        }
        let LocalVersion { hci_version, hci_subversion, lmp_subversion, .. } = hci.read_local_version().await?;
        let chip_type = match lmp_subversion {
            RTL_ROM_LMP_8703B => {
                let [_status, chip_type] = hci.read_reg16(RTL_CPIP_TYPE).await?.to_le_bytes();
                chip_type & 0x0F
            },
            _ => 0
        };
        Ok((lmp_subversion, hci_subversion, hci_version, chip_type))
    }

    async fn try_load_firmware(&self, hci: &Hci) -> Result<bool, Error> {
        //TODO Do the vid/pid check

        let (lmp_subversion, hci_subversion, hci_version, chip_type) = self.find_chip_info(hci).await?;
        let info = DRIVER_INFOS
            .into_iter()
            .find(|info| info.matches(lmp_subversion, hci_subversion, hci_version, HciBus::Usb, chip_type))
            .copied()
            .ok_or(Error::from("Failed to find driver info"))?;
        debug!("found driver info: {:?}", info);

        let firmware_path = find_binary_path(info.firmware_name)
            .ok_or(Error::from("Failed to find firmware file"))?;
        debug!("firmware path: {:?}", firmware_path);
        let firmware = tokio::fs::read(firmware_path)
            .await
            .map_err(|_| Error::from("Failed to find load firmware"))?;



        let config = if !info.config_name.is_empty() {
            let config_path = find_binary_path(info.config_name)
                .ok_or(Error::from("Failed to find config file"))?;
            let config = tokio::fs::read(config_path)
                .await
                .map_err(|_| Error::from("Failed to find load firmware config.bin"))?;
            Some(config)
        } else {
            None
        };
        if config.is_none() && info.config_needed {
            return Err(Error::from("Config needed, but no config file available"));
        }
        //TODO update this code to support other chips as well
        //match info.rom {
        //    RTK_ROM_LMP_8723B | RTK_ROM_LMP_8821A | RTK_ROM_LMP_8761A | RTK_ROM_LMP_8822B | RTK_ROM_LMP_8852A => {
        //        download_for_rtl8723b(hci, info, firmware, config).await.map(|_| true)
        //    },
        //    _ => Err(Error::from("ROM not supported"))
        //}
        Ok(true)
    }
}

impl FirmwareLoader for RealTekFirmwareLoader {
    fn try_load_firmware<'a>(&'a self, host: &'a Hci) -> Pin<Box<dyn Future<Output=Result<bool, Error>> + Send + 'a>> {
        Box::pin(Self::try_load_firmware(self, host))
    }
}

trait RtkHciExit {

    async fn read_rom_version(&self) -> Result<u8, Error>;
    async fn download(&self, index: u8, data: &[u8]) -> Result<u8, Error>;

    async fn read_reg16(&self, cmd: [u8; 5]) -> Result<u16, Error>;

    //async fn core_dump(&self) -> Result<(), Error>;
}

impl RtkHciExit for Hci {
    async fn read_rom_version(&self) -> Result<u8, Error> {
        self.call(Opcode::new(OpcodeGroup::Vendor, 0x006D)).await
    }

    async fn download(&self, index: u8, data: &[u8]) -> Result<u8, Error> {
        self.call_with_args(
            Opcode::new(OpcodeGroup::Vendor, 0x0020),
            |p| {
                p.put_u8(index);
                p.put_slice(data);
            })
            .await
    }

    async fn read_reg16(&self, cmd: [u8; 5]) -> Result<u16, Error> {
        self.call_with_args(Opcode::new(OpcodeGroup::Vendor, 0x0061), |p| {
            p.put_slice(&cmd);
        }).await
    }

    //async fn core_dump(&self) -> Result<(), Error> {
    //    self.call_with_args(Opcode::new(OpcodeGroup::Vendor, 0x00FF), |p| {
    //        p.put_u8(0x00);
    //        p.put_u8(0x00);
    //    }).await?;
    //    Ok(())
    //}

}

const EPATCH_SIGNATURE: &[u8] = b"Realtech";
const EXTENSION_SIGNATURE: &[u8] = &[0x51, 0x04, 0xFD, 0x77];
const EPATCH_HEADER_SIZE: usize = 14;

#[allow(dead_code)]
struct Patch {
    chip_id: u16,
    svn_version: u32,
    data: Vec<u8>,
}

#[allow(dead_code)]
struct Firmware {
    project_id: i32,
    version: u32,
    num_patches: usize,
    patches: Vec<Patch>
}

impl Firmware {

    fn from_bytes(firmware: &[u8]) -> Result<Self, Error> {
        ensure!(firmware.starts_with(EPATCH_SIGNATURE), "Firmware does not start with epatch signature");
        ensure!(firmware.ends_with(EXTENSION_SIGNATURE), "Firmware does not end with extension sig");
        //The firmware should start with a 14 byte header.
        ensure!(firmware.len() >= EPATCH_HEADER_SIZE, "Firmware too short");
        let mut offset = firmware.len() - EXTENSION_SIGNATURE.len();
        let mut project_id = -1;
        while offset >= EPATCH_HEADER_SIZE {
            let length = firmware[offset - 2];
            let opcode = firmware[offset - 1];
            offset -= 2;
            if opcode == 0xFF {
                break;
            }
            ensure!(length > 0, "Invalid 0-length instruction");
            if opcode == 0 && length == 1 {
                project_id = firmware[offset - 1] as i32;
                break;
            }
            offset -= length as usize;
        }

        ensure!(project_id >= 0, "Project ID not found");

        // Read the patch tables info.
        let version = u32::from_le_bytes(read_bytes(firmware, 8));
        let num_patches = u16::from_le_bytes(read_bytes(firmware, 12)) as usize;

        let mut patches = Vec::new();

        // The patches tables are laid out as:
        // <ChipID_1><ChipID_2>...<ChipID_N>  (16 bits each)
        // <PatchLength_1><PatchLength_2>...<PatchLength_N> (16 bits each)
        // <PatchOffset_1><PatchOffset_2>...<PatchOffset_N> (32 bits each)
        ensure!(EPATCH_HEADER_SIZE + 8 * num_patches <= firmware.len(), "Firmware too short");
        let chip_id_table_offset = EPATCH_HEADER_SIZE;
        let patch_length_table_offset = chip_id_table_offset + 2 * num_patches;
        let patch_offset_table_offset = chip_id_table_offset + 4 * num_patches;
        for patch_index in 0..num_patches {
            let chip_id_offset = chip_id_table_offset + 2 * patch_index;
            let chip_id = u16::from_le_bytes(read_bytes(firmware, chip_id_offset));
            let patch_length = u16::from_le_bytes(read_bytes(firmware, patch_length_table_offset + 2 * patch_index)) as usize;
            let patch_offset = u32::from_le_bytes(read_bytes(firmware, patch_offset_table_offset + 4 * patch_index)) as usize;
            ensure!(patch_offset + patch_length <= firmware.len(), "Firmware too short");

            // Get the SVN version for the patch
            let svn_version = u32::from_le_bytes(read_bytes(firmware, patch_offset + patch_length - 8));
            // Create a payload with the patch, replacing the last 4 bytes with the firmware version.
            patches.push(Patch {
                chip_id,
                svn_version,
                data: firmware[patch_offset..patch_offset + patch_length - 4]
                    .iter()
                    .chain(version.to_le_bytes().iter())
                    .copied()
                    .collect(),
            })
        }

        Ok(Self {
            project_id,
            version,
            num_patches,
            patches,
        })
    }

}

fn read_bytes<const N: usize>(data: &[u8], offset: usize) -> [u8; N] {
    let mut result = [0; N];
    result.copy_from_slice(&data[offset..offset + N]);
    result
}