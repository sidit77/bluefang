use std::array::from_fn;
use std::collections::VecDeque;
use std::iter::{repeat, zip};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool};
use std::time::Duration;

use anyhow::Context;
use bytes::{Bytes, BytesMut};
use cpal::{default_host, SampleFormat, Stream};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use instructor::BufferMut;
use ringbuf::{HeapProd, HeapRb};
use ringbuf::consumer::Consumer;
use ringbuf::producer::Producer;
use ringbuf::traits::{Observer, Split};
use rubato::{FftFixedIn, Resampler};
use sbc_rs::Decoder;
use tokio::signal::ctrl_c;
use tokio::time::{Instant};
use tracing::{error, info, trace};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use bluefang::a2dp::SbcMediaCodecInformationRaw;
use bluefang::avdtp::{AvdtpServerBuilder, LocalEndpoint, StreamHandler};
use bluefang::avdtp::packets::{AudioCodec, MediaType, ServiceCategory, StreamEndpointType};

use bluefang::firmware::RealTekFirmwareLoader;
use bluefang::hci::connection::ConnectionManagerBuilder;
use bluefang::hci::consts::{ClassOfDevice, MajorDeviceClass, MajorServiceClasses};
use bluefang::hci::Hci;
use bluefang::host::usb::UsbController;
use bluefang::l2cap::{AVDTP_PSM, L2capServerBuilder, SDP_PSM};
use bluefang::sdp::SdpServer;


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(layer().without_time())
        .with(EnvFilter::from_default_env())
        .init();

    //return play_saved_audio2().await;

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
        let _l2cap_server = L2capServerBuilder::default()
            .with_server(SDP_PSM, SdpServer::default())
            .with_server(AVDTP_PSM, AvdtpServerBuilder::default()
                .with_endpoint(LocalEndpoint {
                    media_type: MediaType::Audio,
                    seid: 1,
                    in_use: Arc::new(AtomicBool::new(false)),
                    tsep: StreamEndpointType::Sink,
                    capabilities: vec![
                        (ServiceCategory::MediaTransport, Bytes::new()),
                        (ServiceCategory::MediaCodec, {
                            let mut codec = BytesMut::new();
                            codec.write_be(&((MediaType::Audio as u8) << 4));
                            codec.write_be(&AudioCodec::Sbc);
                            codec.write_be(&SbcMediaCodecInformationRaw {
                                sampling_frequency: u8::MAX,
                                channel_mode: u8::MAX,
                                block_length: u8::MAX,
                                subbands: u8::MAX,
                                allocation_method: u8::MAX,
                                minimum_bitpool: 2,
                                maximum_bitpool: 53,
                            });
                            codec.freeze()
                        }),
                    ],
                    //stream_handler_factory: Box::new(|cap| Box::new(FileDumpHandler::new())),
                    stream_handler_factory: Box::new(|cap| Box::new(SbcStreamHandler::new(cap))),
                })
                .build())
            .spawn(host.clone())?;

        host.write_local_name("redtest").await?;
        host.write_class_of_device(cod).await?;
        host.set_scan_enabled(true, true).await?;

        tokio::signal::ctrl_c().await?;
    }
    host.shutdown().await?;
    Ok(())

}


struct SbcStreamHandler {
    audio_session: AudioSession,
    decoder: Decoder,
}

impl SbcStreamHandler {

    pub fn new(capabilities: &[(ServiceCategory, Bytes)]) -> Self {
        Self {
            audio_session: AudioSession::new().unwrap(),
            decoder: Decoder::new(Vec::new()),
        }
    }

    fn process_frames(&mut self, data: &[u8]) {
        println!("buffer: {}", self.audio_session.writer().occupied_len());
        self.decoder.refill_buffer(data);
        while let Some(sample) = self.decoder.next_frame() {
            self.audio_session.writer().push_slice(&sample);
        }
    }

    fn pad_stream(&mut self, len: usize) {
        self.audio_session.writer().push_iter(repeat(0).take(len));
    }

}

impl StreamHandler for SbcStreamHandler {
    fn on_reconfigure(&mut self, capabilities: &[(ServiceCategory, Bytes)]) {
        todo!()
    }

    fn on_play(&mut self) {
        self.audio_session.play();
    }

    fn on_stop(&mut self) {
        self.audio_session.stop();
    }

    fn on_data(&mut self, data: Bytes) {
        self.process_frames(&data.as_ref()[1..]);
    }
}

pub struct AudioSession {
    stream: Stream,
    buffer: HeapProd<i16>,
}

impl AudioSession {
    pub fn new() -> anyhow::Result<Self> {
        let host = default_host();
        let device = host
            .default_output_device()
            .context("failed to find output device")?;

        let config = device.supported_output_configs()?
            .inspect(|config| trace!("supported output config: {:?}", config))
            .find(|config| config.sample_format() == SampleFormat::I16 && config.channels() == 2)
            .context("failed to find output config")?
            .with_max_sample_rate()
            .config();
        trace!("selected output config: {:?}", config);

        let buffer: Arc<HeapRb<i16>> = Arc::new(HeapRb::new(100_000_000));
        let (buffer, mut consumer) = buffer.split();

        let stream = device.build_output_stream(
            &config,
            move |data: &mut [i16], _info| {
                let len = consumer.pop_slice(data);
                //data[..len].iter_mut().for_each(|d| *d *=  8);
                data[len..].fill(0);
            },
            move |err| {
                error!("an error occurred on the output stream: {}", err);
            },
            None,
        )?;

        Ok(Self {
            stream,
            buffer,
        })
    }

    pub fn play(&self) {
        self.stream.play().unwrap();
    }

    pub fn stop(&self) {
        self.stream.pause().unwrap();
    }

    pub fn writer(&mut self) -> &mut HeapProd<i16> {
        &mut self.buffer
    }

}

#[allow(dead_code)]
async fn play_saved_audio2() -> anyhow::Result<()> {
    let mut session = SbcStreamHandler::new(&[]);
    let mut file = Bytes::from(std::fs::read("output.sbc")?);


    session.on_play();
    session.pad_stream(512);
    while !file.is_empty() {
        session.process_frames(&file.split_to(8 * 119));
        std::thread::sleep(std::time::Duration::from_millis(21));
    }

    ctrl_c().await?;
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

    let file = std::fs::read("output.sbc")?;
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
        println!("max: {}", sample[0].iter().max().unwrap())
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
struct SbcAudioSink {
    config: StreamConfig,
    stream: Stream,
    capabilities: Vec<(ServiceCategory, Bytes)>,
    buffer: Arc<HeapRb<i16>>
}

impl SbcAudioSink {

    pub fn new(device: &Device) -> anyhow::Result<Self> {
        let config = device.supported_output_configs()?
            .inspect(|config| println!("supported output config: {:?}", config))
            .find(|config| config.sample_format() == SampleFormat::I16 && config.channels() == 2)
            .context("failed to find output config")?
            .with_max_sample_rate()
            .config();
        println!("output config: {:?}", config);

        let buffer: Arc<HeapRb<i16>> = Arc::new(HeapRb::new(8192));
        let mut consumer = CachingCons::new(buffer.clone());

        let stream = device.build_output_stream(
            &config,
            move |data: &mut [i16], _info| {
                let len = consumer.pop_slice(data);
                data[len..].fill(0);
                //data.into_iter().for_each(|d| *d = queue.pop_front().unwrap_or(0));
            },
            move |err| {
                eprintln!("an error occurred on the output stream: {}", err);
            },
            None,
        )?;

        Ok(Self {
            config,
            stream,
            capabilities: vec![
                (ServiceCategory::MediaTransport, Bytes::new()),
                (ServiceCategory::MediaCodec, {
                    let mut codec = BytesMut::new();
                    codec.write_be(&((MediaType::Audio as u8) << 4));
                    codec.write_be(&AudioCodec::Sbc);
                    codec.write_be(&SbcMediaCodecInformationRaw {
                        sampling_frequency: u8::MAX,
                        channel_mode: u8::MAX,
                        block_length: u8::MAX,
                        subbands: u8::MAX,
                        allocation_method: u8::MAX,
                        minimum_bitpool: 2,
                        maximum_bitpool: 53,
                    });
                    codec.freeze()
                })
            ],
            buffer,
        })
    }
}

impl MediaSink for SbcAudioSink {

    fn media_type(&self) -> MediaType {
        MediaType::Audio
    }

    fn capabilities(&self) -> &[(ServiceCategory, Bytes)] {
        &self.capabilities
    }

    fn in_use(&self) -> bool {
        self.buffer.write_is_held()
    }

    fn start(&self) {
        self.stream.play().unwrap();
    }

    fn stop(&self) {
        self.stream.pause().unwrap();
    }

    fn decoder(&self) -> Box<dyn MediaDecoder> {
        Box::new(SbcAudioDecoder::new(44100, self.config.sample_rate.0 as usize, &self.buffer))
    }
}

struct SbcAudioDecoder {
    decoder: Decoder,
    resampler: FftFixedIn<f32>,
    input_buffers: [Vec<f32>; 2],
    output_buffers: [Vec<f32>; 2],
    queue: HeapProd<i16>,
}

impl SbcAudioDecoder {

    pub fn new(input_rate: usize, output_rate: usize, buffer: &Arc<HeapRb<i16>>) -> Self {
        let decoder = Decoder::new(Vec::new());
        let resampler = FftFixedIn::new(
            input_rate,
            output_rate,
            128,
            1,
            2,
        ).unwrap();
        let input_buffers  : [_; 2] = from_fn(|_| vec![0f32; resampler.input_frames_max()]);
        let output_buffers : [_; 2] = from_fn(|_| vec![0f32; resampler.output_frames_max()]);
        let queue = CachingProd::new(buffer.clone());

        Self {
            decoder,
            resampler,
            input_buffers,
            output_buffers,
            queue,
        }
    }
}

impl MediaDecoder for SbcAudioDecoder {

    fn decode(&mut self, data: Bytes) {
        self.decoder.refill_buffer(data.as_ref());
        while let Some(sample) = self.decoder.next_frame_lr() {
            for (sample, buffer) in zip(sample.into_iter(), self.input_buffers.iter_mut()) {
                buffer.clear();
                buffer.extend(sample.iter().map(|s| *s as f32));
            }
            let (_, len) = self.resampler.process_into_buffer(&mut self.input_buffers, &mut self.output_buffers, None).unwrap();
            for (&l, &r) in zip(&self.output_buffers[0], &self.output_buffers[1]).take(len) {
                let _ = self.queue.try_push((l * 8.0) as i16);
                let _ = self.queue.try_push((r * 8.0) as i16);
            }
        }
    }
}
*/
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