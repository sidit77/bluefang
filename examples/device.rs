use anyhow::Context;
use tracing_subscriber::fmt::layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use redtooth::firmware::RealTekFirmwareLoader;
use redtooth::hci::Hci;
use redtooth::host::usb::UsbController;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(layer().without_time())
        .init();

    Hci::register_firmware_loader(RealTekFirmwareLoader::new());

    let usb = UsbController::list(|info| info.vendor_id() == 0x2B89)?
        .next()
        .context("failed to find device")?
        .claim()?;

    let _host = Hci::new(usb).await?;



    tokio::signal::ctrl_c().await?;

    Ok(())

}


