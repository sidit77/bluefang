use std::sync::Arc;
use anyhow::Context;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use redtooth::firmware::RealTekFirmwareLoader;
use redtooth::hci::connection::handle_connection;
use redtooth::hci::consts::{ClassOfDevice, MajorDeviceClass, MajorServiceClasses};
use redtooth::hci::Hci;
use redtooth::host::usb::UsbController;
use redtooth::l2cap::start_l2cap_server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(layer().without_time())
        .with(EnvFilter::from_default_env())
        .init();

    Hci::register_firmware_loader(RealTekFirmwareLoader::new());

    let usb = UsbController::list(|info| info.vendor_id() == 0x2B89)?
        .next()
        .context("failed to find device")?
        .claim()?;


    let cod = ClassOfDevice {
        major_service_classes: MajorServiceClasses::Audio | MajorServiceClasses::Rendering,
        major_device_classes: MajorDeviceClass::AudioVideo,
        minor_device_classes: 4,
    };
    //let cod = ClassOfDevice::from(2360324);
    println!("Class of Device: {:?}", cod);

    let host = Arc::new(Hci::new(usb).await?);
    {
        handle_connection(host.clone())?;
        start_l2cap_server(host.clone())?;

        //let (acl_in, acl_out) = host.acl().await?;
        //spawn(do_l2cap(acl_in, acl_out));
        host.write_local_name("redtest").await?;
        host.write_class_of_device(cod).await?;
        host.set_scan_enabled(true, true).await?;
        //host.inquiry(Lap::General, 5, 0).await?;

        tokio::signal::ctrl_c().await?;
    }
    host.shutdown().await?;
    Ok(())

}


