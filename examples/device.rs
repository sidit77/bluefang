use std::iter;
use std::mem::{size_of, zeroed};
use std::sync::Arc;

use anyhow::Context;
use cpal::{default_host, SampleFormat};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use libsbc_sys::sbc_struct;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use bluefang::firmware::RealTekFirmwareLoader;
use bluefang::hci::connection::ConnectionManagerBuilder;
use bluefang::hci::consts::{ClassOfDevice, MajorDeviceClass, MajorServiceClasses};
use bluefang::hci::Hci;
use bluefang::host::usb::UsbController;
use bluefang::l2cap::start_l2cap_server;



#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(layer().without_time())
        .with(EnvFilter::from_default_env())
        .init();

    Hci::register_firmware_loader(RealTekFirmwareLoader::new());

    let usb = UsbController::list(|info| info.vendor_id() == 0x2B89 || info.vendor_id() == 0x10D7)?
    //let usb = UsbController::list(|info| info.vendor_id() == 0x10D7)?
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
        let _conn_manager = ConnectionManagerBuilder::default()
            .with_link_key_store("link-keys.dat")
            .spawn(host.clone())
            .await?;
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

#[allow(dead_code)]
async fn play_saved_audio() -> anyhow::Result<()> {
    let host = default_host();
    let device = host
        .default_output_device()
        .context("failed to find output device")?;

    let config = device.supported_output_configs()?
        .find(|config| config.sample_format() == SampleFormat::I16)
        .context("failed to find output config")?
        .with_max_sample_rate()
        .config();

    let file = std::fs::read("./target/sbc/output.sbc")?;
    let mut decoder = Decoder::new(file);
    let mut source = iter::from_fn(move || decoder.next_frame())
        .inspect(|frame| println!("decoded {} samples", frame.len()))
        .flat_map(|frame| frame.into_iter())
        .chain(iter::repeat(0));
    //let mut source = iter::from_fn(move || decoder
    //    .next_frame()
    //    .map_err(|e| eprintln!("error decoding frame: {}", e))
    //    .ok())
    //    .flat_map(|frame| frame.data.into_iter())
    //    .chain(iter::repeat(0));

    //let mut source = (0u64..)
    //    .map(|i| (i as f32 * (100.0 + 200.0 * f32::sin(i as f32 * 0.00001)) * 2.0 * std::f32::consts::PI / 44000.0).sin())
    //    .map(|s| (s * 0.7 * i16::MAX as f32) as i16);

    let stream = device.build_output_stream(
        &config,
        move |data: &mut [i16], _info| {
            println!("playing {} samples", data.len());
            data.into_iter().for_each(|d| *d = source.next().unwrap());
        },
        move |err| {
            eprintln!("an error occurred on the output stream: {}", err);
        },
        None,
    )?;

    stream.play()?;

    tokio::signal::ctrl_c().await?;

    stream.pause()?;

    Ok(())
}

struct Decoder {
    buffer: Vec<u8>,
    index: usize,
    sbc: Box<libsbc_sys::sbc_struct>
}

unsafe impl Send for Decoder {}

impl Decoder {
    pub fn new(data: Vec<u8>) -> Self {
        let mut sbc: Box<sbc_struct> = unsafe { Box::new(zeroed()) };
        unsafe { libsbc_sys::sbc_init(sbc.as_mut(), 0) };
        Self { buffer: data, index: 0, sbc }
    }

    pub fn next_frame(&mut self) -> Option<Vec<i16>> {
        let mut pcm: Vec<i16> = Vec::with_capacity(8196);
        let remaining_buffer = &mut self.buffer[self.index..];

        let mut num_written: usize = 0;
        let num_read: isize = unsafe {
            libsbc_sys::sbc_decode(
                self.sbc.as_mut(),
                remaining_buffer.as_ptr() as *const std::os::raw::c_void,
                remaining_buffer.len(),
                pcm.as_mut_ptr() as *mut std::os::raw::c_void,
                pcm.capacity(),
                &mut num_written,
            ) as _
        };

        if num_written > 0 {
            unsafe { pcm.set_len(num_written / size_of::<i16>()) }
        }
        self.index += usize::try_from(num_read).ok()?;

        (num_written > 0).then_some(pcm)
    }

}
