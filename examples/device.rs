use anyhow::Context;
use nusb::{Device, Interface, list_devices};
use nusb::descriptors::{InterfaceAltSetting};
use nusb::transfer::{ControlOut, ControlType, Recipient, RequestBuffer};
use tracing::{debug, info, warn};
use tracing_subscriber::fmt::layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use redtooth::ensure;
use redtooth::utils::IteratorExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(layer().without_time())
        .init();

    for device in list_devices()?
       // .filter(|d| d.vendor_id() == 0x2b89)
    {
        //info!("{:#?}", device);
        let device = device.open()?;
        if let Some(e) = Endpoints::discover(&device) {
            info!("{:#?}", e);
        }
    }

    let usb = list_devices()?
        .filter_map(|info| info
            .open()
            .map_err(|e| warn!("Failed to open device ({e})"))
            .ok())
        .filter_map(|device| Endpoints::discover(&device)
            .map(|endpoints| UsbController { device, endpoints }))
        .next()
        .context("failed to find device")?
        .claim()?;

    let mut events = usb.interface.interrupt_in_queue(usb.endpoints.event);

    tokio::spawn(async move {
        for _ in 0..2 {
            events.submit(RequestBuffer::new(255));
        }
        loop {
            let event = events.next_complete().await;
            event.status.unwrap();
            println!("{:?}", event.data);
            events.submit(RequestBuffer::reuse(event.data, 255));
        }
    });

    let mut cmd = [0u8; 3];
    cmd[..2].copy_from_slice(&OpcodeGroup::HciControl.ocf(0x0003).to_le_bytes());

    let cmd = usb.interface.control_out(ControlOut {
        control_type: ControlType::Class,
        recipient: Recipient::Interface,
        request: 0x00,
        value: 0x00,
        index: usb.endpoints.main_iface.into(),
        data: &cmd,
    }).await;
    cmd.status.unwrap();
    println!("CMD result: {:?}", cmd.data.reuse());

    tokio::signal::ctrl_c().await?;

    Ok(())

}

// Opcode group field definitions.
#[derive(Clone, Copy)]
#[repr(u16)]
enum OpcodeGroup {
    LinkControl = 0x01,
    LinkPolicy = 0x02,
    HciControl = 0x03,
    InfoParams = 0x04,
    StatusParams = 0x05,
    Testing = 0x06,
    Le = 0x08,
    Vendor = 0x3F, // [Vol 4] Part E, Section 5.4.1
}

impl OpcodeGroup {
    /// Combines OGF with OCF to create a full opcode.
    #[inline]
    const fn ocf(self, ocf: u16) -> u16 {
        (self as u16) << 10 | ocf
    }
}

pub struct UsbController {
    device: Device,
    endpoints: Endpoints
}

impl UsbController {
    pub fn claim(self) -> anyhow::Result<UsbHost> {
        debug!("Claiming main interface");
        let interface = self.device.detach_and_claim_interface(self.endpoints.main_iface)?;
        Ok(UsbHost {
            device: self.device,
            endpoints: self.endpoints,
            interface,
        })
    }
}

pub struct UsbHost {
    device: Device,
    endpoints: Endpoints,
    interface: Interface
}

/// USB addresses for Bluetooth interfaces and endpoints ([Vol 4] Part B, Section 2.1.1).
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
struct Endpoints {
    main_iface: u8,
    event: u8,
    acl_out: u8,
    acl_in: u8,
}

impl Endpoints {

    fn discover(dev: &Device) -> Option<Self> {
       dev
           .active_configuration()
           .map_err(|e| warn!("Failed to get config descriptor ({e})"))
           .ok()?
           .interfaces()
            .filter_map(|ifg| {
                let ifas = ifg.alt_settings().single().filter(Self::is_bluetooth)?;
                ensure!(ifas.alternate_setting() == 0 && ifas.num_endpoints() == 3);

                let mut r = Endpoints { main_iface: ifas.interface_number(), event: 0, acl_out: 0, acl_in: 0 };
                for epd in ifas.endpoints() {
                    use nusb::transfer::{Direction::*, EndpointType::*};
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