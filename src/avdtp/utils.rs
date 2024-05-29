use std::io::Write;
use bytes::Bytes;
use tracing::debug;
use crate::avdtp::packets::ServiceCategory;
use crate::avdtp::StreamHandler;

pub struct DebugStreamHandler;

impl StreamHandler for DebugStreamHandler {
    fn on_reconfigure(&mut self, capabilities: &[(ServiceCategory, Bytes)]) {
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
    pub fn new() -> Self {
        Self {
            file: std::fs::File::create("output.sbc").unwrap(),
            total: 0,
        }
    }
}

impl StreamHandler for FileDumpHandler {
    fn on_reconfigure(&mut self, _capabilities: &[(ServiceCategory, Bytes)]) {

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
