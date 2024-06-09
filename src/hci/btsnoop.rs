use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::thread::{spawn, JoinHandle};
use std::time::{Duration, SystemTime};

use bytes::Bytes;
use tracing::{error, info};

const BTSNOOP_MAGIC: &[u8] = b"btsnoop\0";
const BTSNOOP_VERSION: u32 = 1;

// const BTSNOOP_FORMAT_HCI: u32 = 1001;
const BTSNOOP_FORMAT_MONITOR: u32 = 2001;

pub struct LogWriter {
    sender: Option<Sender<(SystemTime, PacketType, Bytes)>>,
    thread: Option<JoinHandle<()>>
}

impl LogWriter {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        match std::env::var_os("BTSNOOP_LOG").map(PathBuf::from) {
            Some(path) => {
                let (sender, receiver) = std::sync::mpsc::channel();
                let thread = spawn(move || {
                    Self::writer_thread(path, receiver).unwrap_or_else(|err| error!("Failed to write btsnoop log: {:?}", err));
                });

                Self {
                    sender: Some(sender),
                    thread: Some(thread)
                }
            }
            None => Self { sender: None, thread: None }
        }
    }

    fn writer_thread(path: PathBuf, receiver: Receiver<(SystemTime, PacketType, Bytes)>) -> std::io::Result<()> {
        let mut file = BufWriter::new(File::create(&path)?);
        info!("Writing btsnoop log to {:?}", path);
        file.write_all(BTSNOOP_MAGIC)?;
        file.write_all(&BTSNOOP_VERSION.to_be_bytes())?;
        file.write_all(&BTSNOOP_FORMAT_MONITOR.to_be_bytes())?;
        file.flush()?;

        while let Ok((timestamp, packet_type, data)) = receiver.recv() {
            const THIRTY_YEARS: Duration = Duration::from_secs(946684800);
            let timestamp = timestamp
                .duration_since(SystemTime::UNIX_EPOCH)
                .ok()
                .and_then(|d| d.checked_sub(THIRTY_YEARS))
                .unwrap_or_default()
                .as_micros() as i64;
            let size = data.len() as u32;
            file.write_all(&size.to_be_bytes())?;
            file.write_all(&size.to_be_bytes())?;
            file.write_all(&(packet_type as u32).to_be_bytes())?;
            file.write_all(&0u32.to_be_bytes())?; // dropped packets
            file.write_all(&(timestamp + 0x00E03AB44A676000).to_be_bytes())?;
            file.write_all(&data)?;
            file.flush()?;
        }

        Ok(())
    }

    pub fn write(&self, packet_type: PacketType, data: Bytes) {
        if let Some(sender) = &self.sender {
            let _ = sender.send((SystemTime::now(), packet_type, data));
        }
    }
}

impl Drop for LogWriter {
    fn drop(&mut self) {
        self.sender = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u32)]
pub enum PacketType {
    Command = 0x02,
    Event = 0x03,
    AclTx = 0x04,
    AclRx = 0x05,
    SystemNode = 0x0c
}
