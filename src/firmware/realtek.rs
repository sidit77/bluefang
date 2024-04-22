use std::future::Future;
use std::pin::Pin;
use crate::hci::{Error, FirmwareLoader, Host};

#[derive(Default, Debug, Copy, Clone)]
pub struct RealTekFirmwareLoader;

impl RealTekFirmwareLoader {
    pub fn new() -> Self {
        Self::default()
    }

    async fn try_load_firmware(&self, host: &Host) -> Result<bool, Error> {
        todo!()
    }
}

impl FirmwareLoader for RealTekFirmwareLoader {
    fn try_load_firmware<'a>(&'a self, host: &'a Host) -> Pin<Box<dyn Future<Output=Result<bool, Error>> + Send + 'a>> {
        Box::pin(Self::try_load_firmware(self, host))
    }
}