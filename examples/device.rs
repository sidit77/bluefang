use std::array::from_fn;
use std::iter::zip;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool};

use anyhow::Context;
use bytes::{Bytes};
use cpal::{default_host, SampleFormat, Stream, StreamConfig};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::{HeapProd, HeapRb};
use ringbuf::consumer::Consumer;
use ringbuf::producer::Producer;
use ringbuf::traits::{Split};
use rubato::{FastFixedIn, PolynomialDegree, Resampler};
use sbc_rs::Decoder;
use tracing::{error, info, trace};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use bluefang::a2dp::sbc::SbcMediaCodecInformation;
use bluefang::a2dp::sdp::A2dpSinkServiceRecord;
use bluefang::avdtp::{AvdtpBuilder, LocalEndpoint, StreamHandler};
use bluefang::avdtp::capabilities::{Capability, MediaCodecCapability};
use bluefang::avdtp::error::ErrorCode;
use bluefang::avdtp::packets::{MediaType, StreamEndpointType};
use bluefang::avrcp::{AvrcpBuilder};
use bluefang::avrcp::sdp::{AvrcpControllerServiceRecord, AvrcpTargetServiceRecord};

use bluefang::firmware::RealTekFirmwareLoader;
use bluefang::hci::connection::ConnectionManagerBuilder;
use bluefang::hci::consts::{ClassOfDevice, MajorDeviceClass, MajorServiceClasses};
use bluefang::hci::Hci;
use bluefang::host::usb::UsbController;
use bluefang::l2cap::{L2capServerBuilder};
use bluefang::sdp::SdpBuilder;


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(layer().without_time())
        .with(EnvFilter::from_default_env())
        .init();

    //return play_saved_audio2().await;

    Hci::register_firmware_loader(RealTekFirmwareLoader::default()).await;

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

    let host = Arc::new(Hci::new(usb).await?);
    info!("Local BD_ADDR: {}", host.read_bd_addr().await?);
    {
        let _conn_manager = ConnectionManagerBuilder::default()
            .with_link_key_store("link-keys.dat")
            .spawn(host.clone())
            .await?;
        let _l2cap_server = L2capServerBuilder::default()
            .with_protocol(SdpBuilder::default()
                .with_record(A2dpSinkServiceRecord::new(0x00010001))
                .with_record(AvrcpControllerServiceRecord::new(0x00010002))
                .with_record(AvrcpTargetServiceRecord::new(0x00010003))
                .build())
            .with_protocol(AvrcpBuilder::default()
                .build())
            .with_protocol(AvdtpBuilder::default()
                .with_endpoint(LocalEndpoint {
                    media_type: MediaType::Audio,
                    seid: 1,
                    in_use: Arc::new(AtomicBool::new(false)),
                    tsep: StreamEndpointType::Sink,
                    capabilities: vec![
                        Capability::MediaTransport,
                        Capability::MediaCodec(SbcMediaCodecInformation::default().into())
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
    resampler: FastFixedIn<f32>,
    decoder: Decoder,
    input_buffers: [Vec<f32>; 2],
    output_buffers: [Vec<f32>; 2],
    interleave_buffer: Vec<i16>
}

impl SbcStreamHandler {

    pub fn new(capabilities: &[Capability]) -> Self {
        let (source_frequency, input_size) = Self::parse_capabilities(capabilities)
            .ok_or(ErrorCode::BadMediaTransportFormat)
            .unwrap();

        let audio_session = AudioSession::new().unwrap();

        let resampler = FastFixedIn::<f32>::new(
            audio_session.config().sample_rate.0 as f64 / source_frequency as f64,
            1.0,
            PolynomialDegree::Septic,
            input_size as usize,
            2,
        ).unwrap();

        Self {
            decoder: Decoder::new(Vec::new()),
            input_buffers:from_fn(|_| vec![0f32; resampler.input_frames_max()]),
            output_buffers: from_fn(|_| vec![0f32; resampler.output_frames_max()]),
            interleave_buffer: Vec::with_capacity(2 * resampler.output_frames_max()),
            audio_session,
            resampler,
        }
    }

    fn parse_capabilities(capabilities: &[Capability]) -> Option<(u32, u32)> {
        let sbc_info = capabilities
            .iter()
            .find_map(|cap| match cap {
                Capability::MediaCodec(MediaCodecCapability::Sbc(info)) => Some(info),
                _ => None
            })?;
        let frequency = sbc_info
            .sampling_frequencies
            .as_value()?;

        let subbands = sbc_info
            .subbands
            .as_value()?;

        let block_length = sbc_info
            .block_lengths
            .as_value()?;

        Some((frequency, subbands * block_length))
    }

    fn process_frames(&mut self, data: &[u8]) {
        //println!("buffer: {}", self.audio_session.writer().occupied_len());
        self.decoder.refill_buffer(data);
        while let Some(sample) = self.decoder.next_frame_lr() {
            for (sample, buffer) in zip(sample.into_iter(), self.input_buffers.iter_mut()) {
                buffer.clear();
                buffer.extend(sample.iter().map(|s| *s as f32));
            }
            let (_, len) = self.resampler.process_into_buffer(&mut self.input_buffers, &mut self.output_buffers, None).unwrap();

            self.interleave_buffer.clear();
            for (&l, &r) in zip(&self.output_buffers[0], &self.output_buffers[1]).take(len) {
                self.interleave_buffer.push((l * 1.0) as i16);
                self.interleave_buffer.push((r * 1.0) as i16);
            }
            self.audio_session.writer().push_slice(&self.interleave_buffer);
        }
    }

}

impl StreamHandler for SbcStreamHandler {

    fn on_play(&mut self) {
        self.audio_session.play();
    }

    fn on_stop(&mut self) {
        self.audio_session.stop();
    }

    fn on_data(&mut self, data: Bytes) {
        //TODO actually parse the header to make sure the packets are not fragmented
        self.process_frames(&data.as_ref()[1..]);
    }
}

pub struct AudioSession {
    stream: Stream,
    config: StreamConfig,
    buffer: HeapProd<i16>,
    max_buffer_size: usize,
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

        let max_buffer_size = (config.sample_rate.0 * config.channels as u32) as usize;
        let buffer: Arc<HeapRb<i16>> = Arc::new(HeapRb::new(max_buffer_size));
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
            config,
            buffer,
            max_buffer_size,
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

    pub fn config(&self) -> &StreamConfig {
        &self.config
    }

    pub fn max_buffer_size(&self) -> usize {
        self.max_buffer_size
    }

}
