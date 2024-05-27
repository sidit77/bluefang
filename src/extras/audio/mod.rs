use std::sync::Arc;
use anyhow::Context;
use cpal::{default_host, SampleFormat};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::{CachingCons, HeapProd, HeapRb};
use ringbuf::consumer::Consumer;
use ringbuf::traits::Observer;
use tokio::sync::mpsc::Sender;
use tokio::task::spawn_local;
use tracing::{error, trace};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum StreamControl {
    Play,
    Stop
}

#[derive(Clone)]
pub struct AudioSession {
    sender: Sender<StreamControl>,
    buffer: Arc<HeapRb<i16>>,
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

        let buffer: Arc<HeapRb<i16>> = Arc::new(HeapRb::new(8192));
        let mut consumer = CachingCons::new(buffer.clone());

        let stream = device.build_output_stream(
            &config,
            move |data: &mut [i16], _info| {
                let len = consumer.pop_slice(data);
                data[len..].fill(0);
            },
            move |err| {
                error!("an error occurred on the output stream: {}", err);
            },
            None,
        )?;

        let (tx, mut rx) = tokio::sync::mpsc::channel(3);

        spawn_local(async move {
            while let Some(control) = rx.recv().await {
                match control {
                    StreamControl::Play => stream.play().unwrap(),
                    StreamControl::Stop => stream.pause().unwrap()
                }
            }
        });


        Ok(Self {
            sender: tx,
            buffer,
        })
    }

    pub fn play(&self) {
        self.sender.blocking_send(StreamControl::Play).unwrap();
    }

    pub fn stop(&self) {
        self.sender.blocking_send(StreamControl::Stop).unwrap();
    }

    pub fn in_use(&self) -> bool {
        self.buffer.write_is_held()
    }

    pub fn take_write_control(&self) -> HeapProd<i16> {
        HeapProd::new(self.buffer.clone())
    }

}