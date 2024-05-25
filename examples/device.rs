use std::array::from_fn;
use std::collections::VecDeque;
use std::iter::zip;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use cpal::{default_host, SampleFormat};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rubato::{FftFixedIn, Resampler};
use sbc_rs::Decoder;
use tokio::time::Instant;
use tracing::info;
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

    //return play_saved_audio().await;

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
    info!("Local BD_ADDR: {}", host.read_bd_addr().await?);
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
        .inspect(|config| println!("supported output config: {:?}", config))
        .find(|config| config.sample_format() == SampleFormat::I16 && config.channels() == 2)
        .context("failed to find output config")?
        .with_max_sample_rate()
        .config();
    println!("output config: {:?}", config);

    let file = std::fs::read("./target/test.sbc")?;
    let mut decoder = Decoder::new(file);
    //let mut source = iter::from_fn(move || decoder.next_frame().map(|s| s.to_vec()))
    //    .flat_map(|frame| frame.into_iter())
    //    .map(|s| s * 8)
    //    .chain(iter::repeat(0));

    //let mut resampler = SincFixedIn::<f32>::new(
    //    config.sample_rate.0 as f64 / 44100.0,
    //    1.0,
    //    SincInterpolationParameters {
    //        sinc_len: 256,
    //        f_cutoff: 0.95,
    //        oversampling_factor: 160,
    //        interpolation: SincInterpolationType::Nearest,
    //        window: WindowFunction::Blackman,
    //    },
    //    128,
    //    2
    //)?;

    //let mut resampler = FastFixedIn::<f32>::new(
    //    config.sample_rate.0 as f64 / 44100.0,
    //    1.0,
    //    PolynomialDegree::Septic,
    //    128,
    //    2,
    //)?;

    let mut resampler = FftFixedIn::<f32>::new(
        44100,
        config.sample_rate.0 as usize,
        128,
        1,
        2,
    )?;


    let mut queue = VecDeque::new();
    let mut input_buffers  : [_; 2] = from_fn(|_| vec![0f32; resampler.input_frames_max()]);
    let mut output_buffers : [_; 2] = from_fn(|_| vec![0f32; resampler.output_frames_max()]);

    let start_time = Instant::now();
    let mut temp_time;
    let mut decode_time = Duration::from_secs(0);
    let mut resample_time = Duration::from_secs(0);
    let mut queue_time = Duration::from_secs(0);

    loop {
        temp_time = Instant::now();
        let Some(sample) = decoder.next_frame_lr() else { break; };
        decode_time += temp_time.elapsed();

        temp_time = Instant::now();
        for (sample, buffer) in zip(sample.into_iter(), input_buffers.iter_mut()) {
            buffer.clear();
            buffer.extend(sample.iter().map(|s| *s as f32));
        }
        queue_time += temp_time.elapsed();

        temp_time = Instant::now();
        let (_, len) = resampler.process_into_buffer(&mut input_buffers, &mut output_buffers, None)?;
        resample_time += temp_time.elapsed();
        temp_time = Instant::now();
        for (&l, &r) in zip(&output_buffers[0], &output_buffers[1]).take(len) {
            queue.push_back((l * 8.0) as i16);
            queue.push_back((r * 8.0) as i16);
        }
        queue_time += temp_time.elapsed();
    }
    let total_time = start_time.elapsed();

    println!("done processing samples ({}ms):\n\tdecode: {}%\n\tresample: {}%\n\tqueues: {}%",
        total_time.as_secs_f64() * 1000.0,
        (decode_time.as_secs_f64() / total_time.as_secs_f64() * 100.0).round(),
        (resample_time.as_secs_f64() / total_time.as_secs_f64() * 100.0).round(),
        (queue_time.as_secs_f64() / total_time.as_secs_f64() * 100.0).round()
    );


    let stream = device.build_output_stream(
        &config,
        move |data: &mut [i16], _info| {
            data.into_iter().for_each(|d| *d = queue.pop_front().unwrap_or(0));
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

/*
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


 */