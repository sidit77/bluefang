use std::sync::Arc;
use anyhow::Context;
use tokio::spawn;
use tracing::debug;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use redtooth::firmware::RealTekFirmwareLoader;
use redtooth::hci::{Hci};
use redtooth::hci::connection::handle_connection;
use redtooth::hci::consts::{ClassOfDevice, MajorDeviceClass, MajorServiceClasses};
use redtooth::host::usb::UsbController;
use redtooth::l2cap;
use redtooth::l2cap::{do_l2cap};

fn check_acl_packet(data: &[u8]) -> anyhow::Result<()> {
    //match l2cap::handle_acl_packet(data)? {
    //    None => debug!("No reply"),
    //    Some(reply) => {
    //        debug!("Reply:");
    //        l2cap::handle_acl_packet(reply.as_ref())?;
    //    }
    //}
    l2cap::handle_acl_packet(data)?;
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(layer().without_time())
        .with(EnvFilter::from_default_env())
        .init();


    check_acl_packet(&[0x01, 0x20, 0x0a, 0x00, 0x06, 0x00, 0x01, 0x00, 0x0a, 0x02, 0x02, 0x00, 0x02, 0x00])?;
    check_acl_packet(&[0x01, 0x20, 0x0a, 0x00, 0x06, 0x00, 0x01, 0x00, 0x0a, 0x03, 0x02, 0x00, 0x03, 0x00])?;
    check_acl_packet(&[0x01, 0x20, 0x0c, 0x00, 0x08, 0x00, 0x01, 0x00, 0x02, 0x04, 0x04, 0x00, 0x01, 0x00, 0x51, 0x00])?;
    check_acl_packet(&[0x01, 0x20, 0x10, 0x00, 0x0c, 0x00, 0x01, 0x00, 0x04, 0x05, 0x08, 0x00, 0x41, 0x00, 0x00, 0x00, 0x01, 0x02, 0x00, 0x04])?;
    return Ok(());
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
        let (acl_in, acl_out) = host.acl().await?;
        spawn(do_l2cap(acl_in, acl_out));
        host.write_local_name("redtest").await?;
        host.write_class_of_device(cod).await?;
        host.set_scan_enabled(true, true).await?;
        //host.inquiry(Lap::General, 5, 0).await?;

        handle_connection(host.clone()).await?;

        tokio::signal::ctrl_c().await?;
    }
    host.reset().await?;
    Ok(())

}


