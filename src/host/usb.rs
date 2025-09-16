use std::fmt::Debug;
use nusb::descriptors::InterfaceAltSetting;
use nusb::transfer::Direction::{In, Out};
use nusb::transfer::EndpointType::{Bulk, Interrupt};
use nusb::{Device, DeviceInfo, Error, Interface};
use tracing::{debug, warn};

use crate::ensure;
use crate::utils::IteratorExt;

pub struct UsbController {
    device: Device,
    endpoints: Endpoints
}

impl UsbController {
    pub fn list<F: FnMut(&DeviceInfo) -> bool>(filter: F) -> Result<impl Iterator<Item = UsbController>, Error> {
        Ok(nusb::list_devices()?
            .filter(filter)
            .filter_map(|info| {
                debug!("Attempting to open: {} ({:04X} {:04X})", info.product_string().unwrap_or("<unknown>"), info.vendor_id(), info.product_id());
                info.open()
                    .map_err(|e| warn!("Failed to open device ({e})"))
                    .ok()
            })
            .filter_map(|device| Endpoints::discover(&device).map(|endpoints| UsbController { device, endpoints })))
    }

    pub fn claim(self) -> Result<UsbHost, Error> {
        debug!("Claiming main interface");
        let interface = self
            .device
            .detach_and_claim_interface(self.endpoints.main_iface)?;
        Ok(UsbHost {
            device: self.device,
            endpoints: self.endpoints,
            interface
        })
    }
}

pub struct UsbHost {
    pub device: Device,
    pub endpoints: Endpoints,
    pub interface: Interface
}

/// USB addresses for Bluetooth interfaces and endpoints ([Vol 4] Part B, Section 2.1.1).
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Endpoints {
    pub main_iface: u8,
    pub event: u8,
    pub acl_out: u8,
    pub acl_in: u8
}

impl Endpoints {
    fn discover(dev: &Device) -> Option<Self> {
        dev.active_configuration()
            .map_err(|e| warn!("Failed to get config descriptor ({e})"))
            .ok()?
            .interfaces()
            .filter_map(|ifg| {
                let ifas = ifg.alt_settings().single().filter(Self::is_bluetooth)?;
                ensure!(ifas.alternate_setting() == 0 && ifas.num_endpoints() == 3);

                let mut r = Endpoints {
                    main_iface: ifas.interface_number(),
                    event: 0,
                    acl_out: 0,
                    acl_in: 0
                };
                for epd in ifas.endpoints() {
                    match (epd.transfer_type(), epd.direction()) {
                        (Interrupt, In) => r.event = epd.address(),
                        (Bulk, In) => r.acl_in = epd.address(),
                        (Bulk, Out) => r.acl_out = epd.address(),
                        _ => {
                            warn!("Unexpected endpoint: {epd:?}");
                            return None;
                        }
                    }
                }
                Some(r)
            })
            .next()
    }

    fn is_bluetooth(ifas: &InterfaceAltSetting) -> bool {
        // [Vol 4] Part B, Section 3.1
        ifas.class() == 0xE0 && ifas.subclass() == 0x01 && ifas.protocol() == 0x01
    }
}
