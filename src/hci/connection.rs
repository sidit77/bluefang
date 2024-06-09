use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bytes::{Bytes, BytesMut};
use instructor::{Buffer, BufferMut};
use tokio::sync::mpsc::unbounded_channel;
use tokio::task::{spawn_blocking, JoinHandle};
use tokio::{fs, spawn};
use tracing::{debug, trace, warn};

use crate::ensure;
use crate::hci::consts::*;
use crate::hci::{Error, Hci};

#[derive(Debug, Clone)]
pub struct ConnectionManagerBuilder {
    link_key_store: PathBuf,
    simple_secure_pairing: bool
}

impl Default for ConnectionManagerBuilder {
    fn default() -> Self {
        Self {
            link_key_store: PathBuf::from("link-keys.dat"),
            simple_secure_pairing: true
        }
    }
}

impl ConnectionManagerBuilder {
    pub fn with_link_key_store<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.link_key_store = PathBuf::from(path.as_ref());
        self
    }

    pub fn with_simple_secure_pairing(mut self, simple_secure_pairing: bool) -> Self {
        self.simple_secure_pairing = simple_secure_pairing;
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
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
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
                    EventCode::LinkKeyRequest,
                    EventCode::IoCapabilityRequest,
                    EventCode::IoCapabilityResponse,
                    EventCode::UserConfirmationRequest,
                    EventCode::UserPasskeyNotification,
                    EventCode::UserPasskeyRequest,
                    EventCode::KeypressNotification,
                    EventCode::RemoteOobDataRequest,
                    EventCode::SimplePairingComplete
                ],
                tx
            )?;
            rx
        };

        if self.simple_secure_pairing {
            hci.set_simple_pairing_support(true).await?;
        }

        let mut state = ConnectionManagerState {
            hci,
            link_key_store: self.link_key_store,
            link_keys
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
                self.hci
                    .accept_connection_request(addr, Role::Slave)
                    .await?;
            }
            EventCode::PinCodeRequest => {
                // ([Vol 4] Part E, Section 7.7.22).
                let addr: RemoteAddr = data.read_le()?;
                data.finish()?;

                debug!("Pin code request: {}", addr);
                self.hci.pin_code_request_reply(addr, "0000").await?;
            }
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
            }
            EventCode::IoCapabilityRequest => {
                // ([Vol 4] Part E, Section 7.7.40).
                let addr: RemoteAddr = data.read_le()?;
                data.finish()?;

                debug!("Io capability request: {}", addr);
                self.hci
                    .io_capability_reply(
                        addr,
                        IoCapability::NoInputNoOutput,
                        OobDataPresence::NotPresent,
                        AuthenticationRequirements::DedicatedBondingProtected
                    )
                    .await?;
            }
            EventCode::IoCapabilityResponse => {
                // ([Vol 4] Part E, Section 7.7.30).
                let addr: RemoteAddr = data.read_le()?;
                let io: IoCapability = data.read_le()?;
                let oob: bool = data.read_le()?;
                let auth: AuthenticationRequirements = data.read_le()?;
                data.finish()?;

                debug!("Io capability response: {} {:?} {} {:?}", addr, io, oob, auth);
            }
            EventCode::UserConfirmationRequest => {
                // ([Vol 4] Part E, Section 7.7.31).
                let addr: RemoteAddr = data.read_le()?;
                let passkey: u32 = data.read_le()?;
                ensure!(passkey <= 999999, "Invalid passkey");
                data.finish()?;

                debug!("User confirmation request: {} {}", addr, passkey);
                self.hci.user_confirmation_request_accept(addr).await?;
            }
            EventCode::SimplePairingComplete => {
                // ([Vol 4] Part E, Section 7.7.45).
                let status: Status = data.read_le()?;
                let addr: RemoteAddr = data.read_le()?;
                data.finish()?;

                debug!("Simple pairing complete: {} {}", addr, status);
            }

            EventCode::UserPasskeyNotification => {
                // ([Vol 4] Part E, Section 7.7.48).
                let addr: RemoteAddr = data.read_le()?;
                let passkey: u32 = data.read_le()?;
                ensure!(passkey <= 999999, "Invalid passkey");
                data.finish()?;

                debug!("User passkey notification: {} {}", addr, passkey);
                panic!("Passkeys not supported");
            }
            EventCode::UserPasskeyRequest => {
                // ([Vol 4] Part E, Section 7.7.43).
                let addr: RemoteAddr = data.read_le()?;
                data.finish()?;

                debug!("User passkey request: {}", addr);
                panic!("Passkeys not supported");
            }
            EventCode::KeypressNotification => {
                // ([Vol 4] Part E, Section 7.7.49).
                let addr: RemoteAddr = data.read_le()?;
                let ty: KeypressNotificationType = data.read_le()?;
                data.finish()?;

                debug!("Keypress notification: {} {:?}", addr, ty);
            }
            EventCode::RemoteOobDataRequest => {
                // ([Vol 4] Part E, Section 7.7.44).
                let addr: RemoteAddr = data.read_le()?;
                data.finish()?;

                debug!("Remote OOB data request: {}", addr);
                panic!("OOB data not supported");
            }
            _ => unreachable!()
        }
        Ok(())
    }

    fn save_link_keys(&self) {
        let mut data = BytesMut::new();
        for (addr, key) in &self.link_keys {
            data.write_le_ref(addr);
            data.write_le_ref(key);
        }
        let data = data.freeze();
        let path = self.link_key_store.clone();
        spawn_blocking(move || std::fs::write(path, &data).unwrap_or_else(|err| warn!("Failed to save link keys: {:?}", err)));
    }
}
