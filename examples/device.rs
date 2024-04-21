use anyhow::Context;
use nusb::transfer::{ControlOut, ControlType, Recipient, RequestBuffer};
use tracing_subscriber::fmt::layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use redtooth::host::usb::UsbController;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(layer().without_time())
        .init();

    let usb = UsbController::list()?
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
#[allow(dead_code)]
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

