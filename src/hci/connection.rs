use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use bytes::{Bytes, BytesMut};
use instructor::{Buffer, BufferMut};
use tokio::{fs, spawn};
use tokio::sync::mpsc::unbounded_channel;
use tokio::task::{JoinHandle, spawn_blocking};
use tracing::{debug, trace, warn};
use crate::ensure;
use crate::hci::consts::{ClassOfDevice, EventCode, LinkKey, LinkKeyType, LinkType, RemoteAddr, Role};
use crate::hci::{Error, Hci};

#[derive(Debug, Clone)]
pub struct ConnectionManagerBuilder {
    link_key_store: PathBuf
}

impl Default for ConnectionManagerBuilder {
    fn default() -> Self {
        Self {
            link_key_store: PathBuf::from("link-keys.dat")
        }
    }
}

impl ConnectionManagerBuilder {
    pub fn with_link_key_store<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.link_key_store = PathBuf::from(path.as_ref());
        self
    }

    pub async fn spawn(self, hci: Arc<Hci>) -> Result<JoinHandle<()>, Error> {
        let link_keys = match fs::read(&self.link_key_store).await {
            Ok(data) => {
                let mut data = data.as_slice();
                let mut result = BTreeMap::new();
                while !data.is_empty() {
                    let addr: RemoteAddr = data.read_le()?;
                    let key: LinkKey = data.read_le()?;
                    result.insert(addr, key);
                }
                result
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                BTreeMap::new()
            },
            Err(err) => return Err(err.into())
        };

        let mut events = {
            let (tx, rx) = unbounded_channel();
            hci.register_event_handler(
                [
                    EventCode::ConnectionRequest,
                    // EventCode::ConnectionComplete,
                    // EventCode::DisconnectionComplete,
                    EventCode::PinCodeRequest,
                    EventCode::LinkKeyNotification,
                    EventCode::LinkKeyRequest
                ],
                tx)?;
            rx
        };

        let mut state = ConnectionManagerState {
            hci,
            link_key_store: self.link_key_store,
            link_keys,
        };

        Ok(spawn(async move {
            while let Some(event) = events.recv().await {
                // trace!("Connection event: {:?}", event);
                state.handle_event(event).await.unwrap_or_else(|err| {
                    warn!("Error handling connection event: {:?}", err);
                });
            }
            trace!("Connection event handler finished");
        }))
    }
}

struct ConnectionManagerState {
    hci: Arc<Hci>,
    link_key_store: PathBuf,
    link_keys: BTreeMap<RemoteAddr, LinkKey>
}

impl ConnectionManagerState {
    async fn handle_event(&mut self, (code, mut data): (EventCode, Bytes)) -> Result<(), Error> {
        match code {
            EventCode::ConnectionRequest => {
                // ([Vol 4] Part E, Section 7.7.4).
                let addr: RemoteAddr = data.read_le()?;
                let _class: ClassOfDevice = data.read_le()?;
                let link_type: LinkType = data.read_le()?;
                data.finish()?;

                ensure!(link_type == LinkType::Acl, "Invalid link type");
                debug!("Connection request: {}", addr);
                self.hci.accept_connection_request(addr, Role::Slave).await?;
            },
            EventCode::PinCodeRequest => {
                // ([Vol 4] Part E, Section 7.7.22).
                let addr: RemoteAddr = data.read_le()?;
                data.finish()?;

                debug!("Pin code request: {}", addr);
                self.hci.pin_code_request_reply(addr, "0000").await?;
            },
            EventCode::LinkKeyRequest => {
                // ([Vol 4] Part E, Section 7.7.23).
                let addr: RemoteAddr = data.read_le()?;
                data.finish()?;

                debug!("Link key request: {}", addr);
                if let Some(key) = self.link_keys.get(&addr) {
                    debug!("   Link key present");
                    self.hci.link_key_present(addr, key).await?;
                } else {
                    debug!("   Link key not present");
                    self.hci.link_key_not_present(addr).await?;
                }
            }
            EventCode::LinkKeyNotification => {
                // ([Vol 4] Part E, Section 7.7.24).
                let addr: RemoteAddr = data.read_le()?;
                let key: LinkKey = data.read_le()?;
                let key_type: LinkKeyType = data.read_le()?;
                data.finish()?;

                debug!("Link key notification: {} {:?} {:?}", addr, key, key_type);
                self.link_keys.insert(addr, key);
                self.save_link_keys();
            },
            _ => unreachable!()
        }
        Ok(())
    }

    fn save_link_keys(&self) {
        let mut data = BytesMut::new();
        for (addr, key) in &self.link_keys {
            data.write_le(addr);
            data.write_le(key);
        }
        let data = data.freeze();
        let path = self.link_key_store.clone();
        spawn_blocking(move || std::fs::write(path, &data)
            .unwrap_or_else(|err| warn!("Failed to save link keys: {:?}", err)));
    }
}

