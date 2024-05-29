use std::io::Write;
use std::path::Path;
use bytes::Bytes;
use tracing::debug;
use crate::avdtp::capabilities::Capability;
use crate::avdtp::StreamHandler;

pub struct DebugStreamHandler;

impl StreamHandler for DebugStreamHandler {
    fn on_reconfigure(&mut self, capabilities: &[Capability]) {
        debug!("Reconfigure: {:?}", capabilities);
    }

    fn on_play(&mut self) {
        debug!("Play");
    }

    fn on_stop(&mut self) {
        debug!("Stop");
    }

    fn on_data(&mut self, data: Bytes) {
        debug!("Data: {} bytes", data.len());
    }
}

pub struct FileDumpHandler {
    file: std::fs::File,
    total: usize
}

impl FileDumpHandler {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            file: std::fs::File::create(path).unwrap(),
            total: 0,
        }
    }
}

impl StreamHandler for FileDumpHandler {
    fn on_reconfigure(&mut self, _capabilities: &[Capability]) {

    }

    fn on_play(&mut self) {

    }

    fn on_stop(&mut self) {

    }

    fn on_data(&mut self, data: Bytes) {
        let data = &data.as_ref()[1..];
        self.file.write_all(data).unwrap();
        self.total += data.len();
        debug!("total: {}", self.total);
    }
}
