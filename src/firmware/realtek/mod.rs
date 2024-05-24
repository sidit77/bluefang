mod commands;
mod info;

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use bytes::BufMut;
use tracing::{debug, error, trace};
use crate::ensure;
use crate::firmware::realtek::commands::{RtkHciExit, RTL_CHIP_REV, RTL_CHIP_SUBVER, RTL_CHIP_TYPE};
use crate::firmware::realtek::info::{DRIVER_INFOS, DriverInfo, HciBus, RTL_ROM_LMP_8703B, RTL_ROM_LMP_8822B};
use crate::hci::{Error, FirmwareLoader, Hci, LocalVersion, Opcode, OpcodeGroup};
use crate::hci::consts::CoreVersion;
use crate::hci::consts::CoreVersion::*;

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
                let [_status, chip_type] = hci.read_reg16(RTL_CHIP_TYPE).await?.to_le_bytes();
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