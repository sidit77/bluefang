#![allow(dead_code)]

use bytes::BufMut;

use crate::hci::{Error, Hci, Opcode, OpcodeGroup};

pub const RTL_CHIP_SUBVER: [u8; 5] = [0x10, 0x38, 0x04, 0x28, 0x80];
pub const RTL_CHIP_REV: [u8; 5] = [0x10, 0x3A, 0x04, 0x28, 0x80];
pub const RTL_SEC_PROJ: [u8; 5] = [0x10, 0xA4, 0x0D, 0x00, 0xb0];
pub const RTL_CHIP_TYPE: [u8; 5] = [0x00, 0x94, 0xa0, 0x00, 0xb0];

pub trait RtkHciExit {
    async fn read_rom_version(&self) -> Result<u8, Error>;
    async fn download(&self, index: u8, data: &[u8]) -> Result<u8, Error>;

    async fn read_reg16(&self, cmd: [u8; 5]) -> Result<u16, Error>;

    async fn drop_firmware(&self) -> Result<(), Error>;

    //async fn core_dump(&self) -> Result<(), Error>;
}

impl RtkHciExit for Hci {
    async fn read_rom_version(&self) -> Result<u8, Error> {
        self.call(Opcode::new(OpcodeGroup::Vendor, 0x006D)).await
    }

    async fn download(&self, index: u8, data: &[u8]) -> Result<u8, Error> {
        self.call_with_args(Opcode::new(OpcodeGroup::Vendor, 0x0020), |p| {
            p.put_u8(index);
            p.put_slice(data);
        })
        .await
    }

    async fn read_reg16(&self, cmd: [u8; 5]) -> Result<u16, Error> {
        self.call_with_args(Opcode::new(OpcodeGroup::Vendor, 0x0061), |p| {
            p.put_slice(&cmd);
        })
        .await
    }

    async fn drop_firmware(&self) -> Result<(), Error> {
        self.call(Opcode::new(OpcodeGroup::Vendor, 0x0066)).await
    }

    //async fn core_dump(&self) -> Result<(), Error> {
    //    self.call_with_args(Opcode::new(OpcodeGroup::Vendor, 0x00FF), |p| {
    //        p.put_u8(0x00);
    //        p.put_u8(0x00);
    //    }).await?;
    //    Ok(())
    //}
}
