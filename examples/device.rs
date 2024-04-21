use anyhow::Context;
use nusb::transfer::{ControlOut, ControlType, Recipient, RequestBuffer};
use tracing_subscriber::fmt::layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use redtooth::hci::consts::Opcode;
use redtooth::hci::Host;
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

    let host = Host::new(usb);
    host.call(Opcode::RESET).await?;

    tokio::signal::ctrl_c().await?;

    Ok(())

}


