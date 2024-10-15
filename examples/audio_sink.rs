use std::array::from_fn;
use std::cmp::PartialEq;
use std::iter::zip;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use bluefang::a2dp::sbc::SbcMediaCodecInformation;
use bluefang::a2dp::sdp::A2dpSinkServiceRecord;
use bluefang::avdtp::capabilities::{Capability, MediaCodecCapability};
use bluefang::avdtp::{AvdtpBuilder, LocalEndpoint, StreamHandler, StreamHandlerFactory, MediaType, StreamEndpointType};
use bluefang::avrcp::notifications::CurrentTrack;
use bluefang::avrcp::sdp::{AvrcpControllerServiceRecord, AvrcpTargetServiceRecord};
use bluefang::avrcp::{Avrcp, AvrcpSession, Event, MediaAttributeId, Notification};
use bluefang::firmware::{FolderFileProvider, RealTekFirmwareLoader};
use bluefang::hci::connection::ConnectionManagerBuilder;
use bluefang::hci::consts::{AudioVideoClass, ClassOfDevice, DeviceClass, MajorServiceClasses};
use bluefang::hci::{FirmwareLoader, Hci};
use bluefang::host::usb::UsbController;
use bluefang::l2cap::L2capServerBuilder;
use bluefang::sdp::SdpBuilder;
use bluefang::utils::{select2, Either2};
use bytes::Bytes;
use console::{Key, Term};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{default_host, SampleFormat, Stream, StreamConfig};
use enum_iterator::{all, Sequence};
use portable_atomic::AtomicF32;
use ringbuf::consumer::Consumer;
use ringbuf::producer::Producer;
use ringbuf::traits::Split;
use ringbuf::{HeapProd, HeapRb};
use rubato::{FastFixedIn, PolynomialDegree, Resampler};
use sbc_rs::BufferedDecoder;
use tokio::spawn;
use tokio::sync::mpsc::Receiver;
use tokio::time::sleep;
use tracing::{error, info, trace, warn};
use tracing_subscriber::fmt::layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;
use bluefang::avc::PassThroughOp;

macro_rules! cloned {
    ([$($vars:ident),+] $e:expr) => {
        {
            $( let $vars = $vars.clone(); )+
            $e
        }
    };
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(layer().without_time())
        .with(EnvFilter::from_default_env())
        .init();

    Hci::register_firmware_loaders([
        RealTekFirmwareLoader::new(FolderFileProvider::new("./firmware")).boxed()
    ]);

    let usb = UsbController::list(|info| info.vendor_id() == 0x2B89 || info.vendor_id() == 0x10D7)?
        .next()
        .context("failed to find device")?
        .claim()?;

    let cod = ClassOfDevice {
        service_classes: MajorServiceClasses::Audio | MajorServiceClasses::Rendering,
        device_class: DeviceClass::AudioVideo(AudioVideoClass::WearableHeadset),
    };
    //let cod = ClassOfDevice::from(2360324);

    let host = Arc::new(Hci::new(usb).await?);
    info!("Local BD_ADDR: {}", host.read_bd_addr().await?);
    {
        let _conn_manager = ConnectionManagerBuilder::default()
            .with_link_key_store("link-keys.dat")
            .spawn(host.clone())
            .await?;
        let volume = Arc::new(AtomicF32::new(1.0));
        let _l2cap_server = L2capServerBuilder::default()
            .with_protocol(
                SdpBuilder::default()
                    .with_record(A2dpSinkServiceRecord::new(0x00010001))
                    .with_record(AvrcpControllerServiceRecord::new(0x00010002))
                    .with_record(AvrcpTargetServiceRecord::new(0x00010003))
                    .build()
            )
            .with_protocol(Avrcp::new(
                cloned!([volume] move |session| avrcp_session_handler(volume.clone(), session))
            ))
            .with_protocol(
                AvdtpBuilder::default()
                    .with_endpoint(LocalEndpoint {
                        media_type: MediaType::Audio,
                        seid: 1,
                        in_use: Arc::new(AtomicBool::new(false)),
                        tsep: StreamEndpointType::Sink,
                        capabilities: vec![
                            Capability::MediaTransport,
                            Capability::MediaCodec(SbcMediaCodecInformation::default().into()),
                        ],
                        //stream_handler_factory: Box::new(|cap| Box::new(FileDumpHandler::new())),
                        factory: StreamHandlerFactory::new(cloned!([volume] move |cap| SbcStreamHandler::new(volume.clone(), cap)))
                    })
                    .build()
            )
            .run(&host)
            .map(spawn)?;

        host.write_local_name("bluefang").await?;
        host.write_class_of_device(cod).await?;
        host.set_scan_enabled(true, true).await?;

        println!("Waiting for connections...");
        println!("Press Ctrl-C to exit");
        tokio::signal::ctrl_c().await?;
    }
    host.shutdown().await?;
    Ok(())
}

fn avrcp_session_handler(volume: Arc<AtomicF32>, mut session: AvrcpSession) {
    spawn(async move {
        session
            .notify_local_volume_change(volume.load(SeqCst))
            .await
            .unwrap_or_else(|err| warn!("Failed to notify volume change: {}", err));
        sleep(Duration::from_millis(200)).await;
        let supported_events = session.get_supported_events().await.unwrap_or_default();
        info!("Supported Events: {:?}", supported_events);
        if supported_events.contains(&CurrentTrack::EVENT_ID) {
            retrieve_current_track_info(&session)
                .await
                .unwrap_or_else(|err| warn!("Failed to retrieve current track info: {}", err));
        }
        let mut commands = command_reader();
        loop {
            match select2(commands.recv(), session.next_event())
                .await
                .transpose()
            {
                Some(Either2::A(command)) => match command {
                    PlayerCommand::Play => {
                        println!("Play");
                        session.action(PassThroughOp::Play).await
                    },
                    PlayerCommand::Pause => {
                        println!("Pause");
                        session.action(PassThroughOp::Pause).await
                    },
                    PlayerCommand::VolumeUp | PlayerCommand::VolumeDown => {
                        let d = if command == PlayerCommand::VolumeUp { 0.1 } else { -0.1 };
                        let _ = volume.fetch_update(SeqCst, SeqCst, |v| Some((v + d).max(0.0).min(1.0)));
                        println!("Volume: {}%", (volume.load(SeqCst) * 100.0).round());
                        session
                            .notify_local_volume_change(volume.load(SeqCst))
                            .await
                    }
                }
                .unwrap_or_else(|err| warn!("Failed to send command: {:?}", err)),
                Some(Either2::B(event)) => match event {
                    Event::TrackChanged(_) => {
                        retrieve_current_track_info(&session)
                            .await
                            .unwrap_or_else(|err| warn!("Failed to retrieve current track info: {}", err));
                    }
                    Event::VolumeChanged(vol) => {
                        volume.store(vol, SeqCst);
                        println!("Volume: {}%", (volume.load(SeqCst) * 100.0).round());
                    },
                    _ => {}
                },
                None => break
            }
        }
    });
}

async fn retrieve_current_track_info(session: &AvrcpSession) -> anyhow::Result<()> {
    let current_track: CurrentTrack = session.register_notification(None).await?;
    match current_track {
        CurrentTrack::NotSelected => println!("No track selected"),
        CurrentTrack::Selected => {
            let attributes = session
                .get_current_media_attributes(Some(&[MediaAttributeId::Title, MediaAttributeId::ArtistName]))
                .await?;
            println!(
                "Current Track: {} - {}",
                attributes
                    .get(&MediaAttributeId::ArtistName)
                    .map_or("", String::as_str),
                attributes
                    .get(&MediaAttributeId::Title)
                    .map_or("", String::as_str)
            );
        }
        CurrentTrack::Id(id) => println!("Track ID: {:?}", id)
    }
    Ok(())
}

struct SbcStreamHandler {
    audio_session: AudioSession,
    resampler: FastFixedIn<f32>,
    decoder: BufferedDecoder,
    volume: Arc<AtomicF32>,
    input_buffers: [Vec<f32>; 2],
    output_buffers: [Vec<f32>; 2],
    interleave_buffer: Vec<i16>
}

impl SbcStreamHandler {
    pub fn new(volume: Arc<AtomicF32>, capabilities: &[Capability]) -> Self {
        let (source_frequency, input_size) = Self::parse_capabilities(capabilities)
            .context("Invalid capabilities")
            .unwrap();

        let audio_session = AudioSession::new().unwrap();

        let resampler = FastFixedIn::<f32>::new(
            audio_session.config().sample_rate.0 as f64 / source_frequency as f64,
            1.0,
            PolynomialDegree::Septic,
            input_size as usize,
            2
        )
        .unwrap();

        Self {
            decoder: BufferedDecoder::default(),
            volume,
            input_buffers: from_fn(|_| vec![0f32; resampler.input_frames_max()]),
            output_buffers: from_fn(|_| vec![0f32; resampler.output_frames_max()]),
            interleave_buffer: Vec::with_capacity(2 * resampler.output_frames_max()),
            audio_session,
            resampler
        }
    }

    fn parse_capabilities(capabilities: &[Capability]) -> Option<(u32, u32)> {
        let sbc_info = capabilities.iter().find_map(|cap| match cap {
            Capability::MediaCodec(MediaCodecCapability::Sbc(info)) => Some(info),
            _ => None
        })?;
        let frequency = sbc_info.sampling_frequencies.as_value()?;

        let subbands = sbc_info.subbands.as_value()?;

        let block_length = sbc_info.block_lengths.as_value()?;

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
            let (_, len) = self
                .resampler
                .process_into_buffer(&mut self.input_buffers, &mut self.output_buffers, None)
                .unwrap();

            self.interleave_buffer.clear();
            let volume = self.volume.load(SeqCst).powi(2);
            for (&l, &r) in zip(&self.output_buffers[0], &self.output_buffers[1]).take(len) {
                self.interleave_buffer.push((l * volume) as i16);
                self.interleave_buffer.push((r * volume) as i16);
            }
            self.audio_session
                .writer()
                .push_slice(&self.interleave_buffer);
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
    max_buffer_size: usize
}

impl AudioSession {
    pub fn new() -> anyhow::Result<Self> {
        let host = default_host();
        let device = host
            .default_output_device()
            .context("failed to find output device")?;

        let config = device
            .supported_output_configs()?
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
            None
        )?;

        Ok(Self {
            stream,
            config,
            buffer,
            max_buffer_size
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

#[derive(Debug, Copy, Clone, PartialEq, Sequence)]
enum PlayerCommand {
    Play,
    Pause,
    VolumeUp,
    VolumeDown
}

impl PlayerCommand {
    fn hotkey(self) -> Key {
        match self {
            PlayerCommand::Play => Key::Char('q'),
            PlayerCommand::Pause => Key::Char('w'),
            PlayerCommand::VolumeUp => Key::Char('e'),
            PlayerCommand::VolumeDown => Key::Char('r')
        }
    }
}

fn command_reader() -> Receiver<PlayerCommand> {
    static IN_USE: AtomicBool = AtomicBool::new(false);
    assert!(!IN_USE.swap(true, SeqCst), "command_reader must be called only once");
    let (tx, rx) = tokio::sync::mpsc::channel(10);
    std::thread::spawn(move || {
        let term = Term::stdout();
        term.write_line("Press 'h' for help").unwrap();
        while let Ok(key) = term.read_key() {
            let command = all::<PlayerCommand>().find(|command| command.hotkey() == key);
            match command {
                Some(command) => {
                    if tx.blocking_send(command).is_err() {
                        break;
                    }
                }
                None if key == Key::Char('h') => {
                    term.write_line("Hotkeys:").unwrap();
                    for command in all::<PlayerCommand>() {
                        term.write_line(&format!("  {:?}: {:?}", command.hotkey(), command))
                            .unwrap();
                    }
                }
                None => continue
            }
        }
        IN_USE.store(false, SeqCst);
    });
    rx
}
