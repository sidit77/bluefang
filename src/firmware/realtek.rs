use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use tracing::{debug, error};
use crate::ensure;
use crate::hci::{Error, FirmwareLoader, Hci, Opcode, OpcodeGroup};
use crate::hci::consts::CoreVersion;
use crate::hci::consts::CoreVersion::*;

const RTK_ROM_LMP_8723A: u16 = 0x1200;
const RTK_ROM_LMP_8723B: u16 = 0x8723;
const RTK_ROM_LMP_8821A: u16 = 0x8821;
const RTK_ROM_LMP_8761A: u16 = 0x8761;
const RTK_ROM_LMP_8822B: u16 = 0x8822;
const RTK_ROM_LMP_8852A: u16 = 0x8852;

#[derive(Debug, Copy, Clone)]
struct DriverInfo {
    rom: u16,
    hci: (u16, CoreVersion),
    config_needed: bool,
    has_rom_version: bool,
    has_msft_ext: bool,
    fw_name: &'static str,
    config_name: &'static str
}

const DRIVER_INFOS: &[DriverInfo] = &[
    DriverInfo {
        rom: RTK_ROM_LMP_8723A,
        hci: (0x0B, V4_0),
        config_needed: false,
        has_rom_version: false,
        has_msft_ext: false,
        fw_name: "rtl8723a_fw.bin",
        config_name: "",
    },
    DriverInfo {
        rom: RTK_ROM_LMP_8723B,
        hci: (0x0B, V4_0),
        config_needed: false,
        has_rom_version: true,
        has_msft_ext: false,
        fw_name: "rtl8723b_fw.bin",
        config_name: "rtl8723b_config.bin",
    },
    DriverInfo {
        rom: RTK_ROM_LMP_8723B,
        hci: (0x0D, V4_2),
        config_needed: true,
        has_rom_version: true,
        has_msft_ext: false,
        fw_name: "rtl8723d_fw.bin",
        config_name: "rtl8723d_config.bin",
    },
    DriverInfo {
        rom: RTK_ROM_LMP_8821A,
        hci: (0x0A, V4_0),
        config_needed: false,
        has_rom_version: true,
        has_msft_ext: false,
        fw_name: "rtl8821a_fw.bin",
        config_name: "rtl8821a_config.bin",
    },
    DriverInfo {
        rom: RTK_ROM_LMP_8821A,
        hci: (0x0C, V4_2),
        config_needed: false,
        has_rom_version: true,
        has_msft_ext: true,
        fw_name: "rtl8821c_fw.bin",
        config_name: "rtl8821c_config.bin",
    },
    DriverInfo {
        rom: RTK_ROM_LMP_8761A,
        hci: (0x0A, V4_0),
        config_needed: false,
        has_rom_version: true,
        has_msft_ext: false,
        fw_name: "rtl8761a_fw.bin",
        config_name: "rtl8761a_config.bin",
    },
    DriverInfo {
        rom: RTK_ROM_LMP_8761A,
        hci: (0x0B, V5_1),
        config_needed: false,
        has_rom_version: true,
        has_msft_ext: false,
        fw_name: "rtl8761bu_fw.bin",
        config_name: "rtl8761bu_config.bin",
    },
    DriverInfo {
        rom: RTK_ROM_LMP_8822B,
        hci: (0x0C, V5_1),
        config_needed: false,
        has_rom_version: true,
        has_msft_ext: true,
        fw_name: "rtl8822cu_fw.bin",
        config_name: "rtl8822cu_config.bin",
    },
    DriverInfo {
        rom: RTK_ROM_LMP_8822B,
        hci: (0x0B, V4_1),
        config_needed: true,
        has_rom_version: true,
        has_msft_ext: true,
        fw_name: "rtl8822b_fw.bin",
        config_name: "rtl8822b_config.bin",
    },
    DriverInfo {
        rom: RTK_ROM_LMP_8852A,
        hci: (0x0A, V5_2),
        config_needed: false,
        has_rom_version: true,
        has_msft_ext: true,
        fw_name: "rtl8852au_fw.bin",
        config_name: "rtl8852au_config.bin",
    },
    DriverInfo {
        rom:RTK_ROM_LMP_8852A,
        hci: (0xB, V5_2),
        config_needed: false,
        has_rom_version: true,
        has_msft_ext: true,
        fw_name: "rtl8852bu_fw.bin",
        config_name: "rtl8852bu_config.bin",
    },
    DriverInfo {
        rom: RTK_ROM_LMP_8852A,
        hci: (0x0C, V5_3),
        config_needed: false,
        has_rom_version: true,
        has_msft_ext: true,
        fw_name: "rtl8852cu_fw.bin",
        config_name: "rtl8852cu_config.bin",
    }
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

#[derive(Default, Debug, Copy, Clone)]
pub struct RealTekFirmwareLoader;

impl RealTekFirmwareLoader {
    pub fn new() -> Self {
        Self::default()
    }

    async fn try_load_firmware(&self, hci: &Hci) -> Result<bool, Error> {
        //TODO Do the vid/pid check
        let local_version = hci.read_local_version().await?;
        let info = DRIVER_INFOS
            .into_iter()
            .find(|info|
                info.rom == local_version.lmp_subversion &&
                    info.hci == (local_version.hci_subversion, local_version.hci_version))
            .copied().ok_or(Error::from("Failed to find driver info"))?;
        debug!("found driver info: {:?}", info);

        let firmware_path = find_binary_path(info.fw_name)
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
                .map_err(|_| Error::from("Failed to find load firmware config"))?;
            Some(config)
        } else {
            None
        };
        if config.is_none() && info.config_needed {
            return Err(Error::from("Config needed, but no config file available"));
        }

        //TODO update this code to support other chips as well
        match info.rom {
            RTK_ROM_LMP_8723B | RTK_ROM_LMP_8821A | RTK_ROM_LMP_8761A | RTK_ROM_LMP_8822B | RTK_ROM_LMP_8852A => {
                download_for_rtl8723b(hci, info, firmware, config).await.map(|_| true)
            },
            _ => Err(Error::from("ROM not supported"))
        }
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

}

impl RtkHciExit for Hci {
    async fn read_rom_version(&self) -> Result<u8, Error> {
        self.call(Opcode::new(OpcodeGroup::Vendor, 0x006D)).await
    }

    async fn download(&self, index: u8, data: &[u8]) -> Result<u8, Error> {
        self.call_with_args(
            Opcode::new(OpcodeGroup::Vendor, 0x0020),
            |p| p.u8(index).bytes(data).end())
            .await
    }
}

const EPATCH_SIGNATURE: &[u8] = b"Realtech";
const EXTENSION_SIGNATURE: &[u8] = &[0x51, 0x04, 0xFD, 0x77];
const EPATCH_HEADER_SIZE: usize = 14;

struct Patch {
    chip_id: u16,
    svn_version: u32,
    data: Vec<u8>,
}

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