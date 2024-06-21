use std::collections::BTreeMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::{Bytes, BytesMut};
use futures_lite::{Stream, StreamExt};
use instructor::{Buffer, BufferMut};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};
use tokio::task::{spawn_blocking, JoinHandle};
use tokio::{fs, spawn};
use tracing::{debug, trace, warn};

use crate::ensure;
use crate::hci::consts::*;
use crate::hci::{Error, Hci};
use crate::utils::catch_error;

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

        let mut events = ConnectionEventReceiver::new(&hci)?;

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
    async fn handle_event(&mut self, event: ConnectionEvent) -> Result<(), Error> {
        match event {
            ConnectionEvent::ConnectionRequest { addr, link_type, .. } => {
                ensure!(link_type == LinkType::Acl, "Invalid link type");
                debug!("Connection request: {}", addr);
                self.hci
                    .accept_connection_request(addr, Role::Slave)
                    .await?;
            }
            ConnectionEvent::PinCodeRequest { addr } => {
                debug!("Pin code request: {}", addr);
                self.hci.pin_code_request_reply(addr, "0000").await?;
            }
            ConnectionEvent::LinkKeyRequest { addr } => {
                debug!("Link key request: {}", addr);
                if let Some(key) = self.link_keys.get(&addr) {
                    debug!("   Link key present");
                    self.hci.link_key_present(addr, key).await?;
                } else {
                    debug!("   Link key not present");
                    self.hci.link_key_not_present(addr).await?;
                }
            }
            ConnectionEvent::LinkKeyNotification { addr, key, key_type } => {
                debug!("Link key notification: {} {:?} {:?}", addr, key, key_type);
                self.link_keys.insert(addr, key);
                self.save_link_keys();
            }
            ConnectionEvent::IoCapabilityRequest { addr} => {
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
            ConnectionEvent::IoCapabilityResponse { addr, io, oob, auth } => {
                debug!("Io capability response: {} {:?} {} {:?}", addr, io, oob, auth);
            }
            ConnectionEvent::UserConfirmationRequest { addr, passkey } => {
                debug!("User confirmation request: {} {}", addr, passkey);
                self.hci.user_confirmation_request_accept(addr).await?;
            }
            ConnectionEvent::SimplePairingComplete { status, addr } => {
                debug!("Simple pairing complete: {} {}", addr, status);
            }
            ConnectionEvent::UserPasskeyNotification { addr, passkey } => {
                debug!("User passkey notification: {} {}", addr, passkey);
                panic!("Passkeys not supported");
            }
            ConnectionEvent::UserPasskeyRequest { addr } => {
                debug!("User passkey request: {}", addr);
                panic!("Passkeys not supported");
            }
            ConnectionEvent::KeypressNotification { addr, ty } => {
                debug!("Keypress notification: {} {:?}", addr, ty);
            }
            ConnectionEvent::RemoteOobDataRequest { addr } => {
                debug!("Remote OOB data request: {}", addr);
                panic!("OOB data not supported");
            },
            _ => {}
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

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ConnectionEvent {
    // ([Vol 4] Part E, Section 7.7.3).
    ConnectionComplete {
        status: Status,
        handle: u16,
        addr: RemoteAddr,
        link_type: LinkType,
        encryption_enabled: bool
    },
    // ([Vol 4] Part E, Section 7.7.4).
    ConnectionRequest {
        addr: RemoteAddr,
        class: ClassOfDevice,
        link_type: LinkType
    },
    // ([Vol 4] Part E, Section 7.7.5).
    DisconnectionComplete {
        status: Status,
        handle: u16,
        reason: Status
    },
    // ([Vol 4] Part E, Section 7.7.7).
    RemoteNameRequestComplete {
        status: Status,
        addr: RemoteAddr,
        name: String
    },
    // ([Vol 4] Part E, Section 7.7.8).
    EncryptionChanged {
        status: Status,
        handle: u16,
        mode: EncryptionMode,
        key_size: Option<u8>
    },
    // ([Vol 4] Part E, Section 7.7.22)
    PinCodeRequest {
        addr: RemoteAddr
    },
    // ([Vol 4] Part E, Section 7.7.23).
    LinkKeyRequest {
        addr: RemoteAddr
    },
    // ([Vol 4] Part E, Section 7.7.24).
    LinkKeyNotification {
        addr: RemoteAddr,
        key: LinkKey,
        key_type: LinkKeyType
    },
    // ([Vol 4] Part E, Section 7.7.40).
    IoCapabilityRequest {
        addr: RemoteAddr
    },
    // ([Vol 4] Part E, Section 7.7.30).
    IoCapabilityResponse {
        addr: RemoteAddr,
        io: IoCapability,
        oob: bool,
        auth: AuthenticationRequirements
    },
    // ([Vol 4] Part E, Section 7.7.31).
    UserConfirmationRequest {
        addr: RemoteAddr,
        passkey: u32
    },
    // ([Vol 4] Part E, Section 7.7.46).
    LinkSuperVisionTimeoutChanged {
        handle: u16,
        timeout: Option<Duration>
    },
    // ([Vol 4] Part E, Section 7.7.48).
    UserPasskeyNotification {
        addr: RemoteAddr,
        passkey: u32
    },
    // ([Vol 4] Part E, Section 7.7.43).
    UserPasskeyRequest {
        addr: RemoteAddr
    },
    // ([Vol 4] Part E, Section 7.7.49).
    KeypressNotification {
        addr: RemoteAddr,
        ty: KeypressNotificationType
    },
    // ([Vol 4] Part E, Section 7.7.44).
    RemoteOobDataRequest {
        addr: RemoteAddr
    },
    // ([Vol 4] Part E, Section 7.7.45).
    SimplePairingComplete {
        status: Status,
        addr: RemoteAddr
    }
}

pub struct ConnectionEventReceiver(UnboundedReceiver<(EventCode, Bytes)>);

impl ConnectionEventReceiver {
    pub fn new(hci: &Hci) -> Result<Self, Error> {
        let events = {
            let (tx, rx) = unbounded_channel();
            hci.register_event_handler(
                [
                    EventCode::ConnectionRequest,
                    EventCode::ConnectionComplete,
                    EventCode::DisconnectionComplete,
                    EventCode::RemoteNameRequestComplete,
                    EventCode::EncryptionChange,
                    EventCode::PinCodeRequest,
                    EventCode::LinkKeyNotification,
                    EventCode::LinkKeyRequest,
                    EventCode::IoCapabilityRequest,
                    EventCode::IoCapabilityResponse,
                    EventCode::UserConfirmationRequest,
                    EventCode::LinkSupervisionTimeoutChanged,
                    EventCode::UserPasskeyNotification,
                    EventCode::UserPasskeyRequest,
                    EventCode::KeypressNotification,
                    EventCode::RemoteOobDataRequest,
                    EventCode::SimplePairingComplete
                ],
                tx
            )?;
            trace!("Registered new connection event listener");
            rx
        };
        Ok(Self(events))
    }

    pub fn recv(&mut self) -> impl Future<Output=Option<ConnectionEvent>> + '_ {
        //poll_fn(move |cx| self.poll_recv(cx))
        self.next()
    }

}

impl Stream for ConnectionEventReceiver {
    type Item = ConnectionEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        while let Poll::Ready(event) = self.0.poll_recv(cx) {
            let Some((code, mut data)) = event else {
                return Poll::Ready(None);
            };
            let event: Result<_, instructor::Error> = catch_error(|| match code {
                EventCode::ConnectionComplete => {
                    let status: Status = data.read_le()?;
                    let handle: u16 = data.read_le()?;
                    let addr: RemoteAddr = data.read_le()?;
                    let link_type: LinkType = data.read_le()?;
                    let encryption_enabled: bool = data.read_le()?;
                    data.finish()?;
                    Ok(ConnectionEvent::ConnectionComplete { status, handle, addr, link_type, encryption_enabled })
                }
                EventCode::DisconnectionComplete => {
                    let status: Status = data.read_le()?;
                    let handle: u16 = data.read_le()?;
                    let reason: Status = data.read_le()?;
                    data.finish()?;
                    Ok(ConnectionEvent::DisconnectionComplete { status, handle, reason })
                },
                EventCode::RemoteNameRequestComplete => {
                    let status: Status = data.read_le()?;
                    let addr: RemoteAddr = data.read_le()?;
                    let name: String = String::from_utf8_lossy(&data.split_to(248))
                        .trim_end_matches('\0')
                        .to_string();
                    data.finish()?;
                    Ok(ConnectionEvent::RemoteNameRequestComplete { status, addr, name })
                }
                EventCode::EncryptionChange | EventCode::EncryptionChangeV2 => {
                    let status: Status = data.read_le()?;
                    let handle: u16 = data.read_le()?;
                    let mode: EncryptionMode = data.read_le()?;
                    let key_size: u8 = if code == EventCode::EncryptionChangeV2 {
                        data.read_le()?
                    } else {
                        0
                    };
                    let key_size = (key_size > 0 && mode != EncryptionMode::Off).then_some(key_size);
                    data.finish()?;
                    Ok(ConnectionEvent::EncryptionChanged { status, handle, mode, key_size})
                }
                EventCode::ConnectionRequest => {
                    let addr: RemoteAddr = data.read_le()?;
                    let class: ClassOfDevice = data.read_le()?;
                    let link_type: LinkType = data.read_le()?;
                    data.finish()?;
                    Ok(ConnectionEvent::ConnectionRequest { addr, class, link_type })
                }
                EventCode::PinCodeRequest => {
                    let addr: RemoteAddr = data.read_le()?;
                    data.finish()?;
                    Ok(ConnectionEvent::PinCodeRequest { addr })
                }
                EventCode::LinkKeyRequest => {
                    let addr: RemoteAddr = data.read_le()?;
                    data.finish()?;
                    Ok(ConnectionEvent::LinkKeyRequest { addr })
                }
                EventCode::LinkKeyNotification => {
                    let addr: RemoteAddr = data.read_le()?;
                    let key: LinkKey = data.read_le()?;
                    let key_type: LinkKeyType = data.read_le()?;
                    data.finish()?;
                    Ok(ConnectionEvent::LinkKeyNotification { addr, key, key_type })
                }
                EventCode::IoCapabilityRequest => {
                    let addr: RemoteAddr = data.read_le()?;
                    data.finish()?;
                    Ok(ConnectionEvent::IoCapabilityRequest { addr })
                }
                EventCode::IoCapabilityResponse => {
                    let addr: RemoteAddr = data.read_le()?;
                    let io: IoCapability = data.read_le()?;
                    let oob: bool = data.read_le()?;
                    let auth: AuthenticationRequirements = data.read_le()?;
                    data.finish()?;
                    Ok(ConnectionEvent::IoCapabilityResponse { addr, io, oob, auth })
                }
                EventCode::UserConfirmationRequest => {
                    let addr: RemoteAddr = data.read_le()?;
                    let passkey: u32 = data.read_le()?;
                    ensure!(passkey <= 999999, instructor::Error::InvalidValue);
                    data.finish()?;
                    Ok(ConnectionEvent::UserConfirmationRequest { addr, passkey })
                }
                EventCode::SimplePairingComplete => {
                    let status: Status = data.read_le()?;
                    let addr: RemoteAddr = data.read_le()?;
                    data.finish()?;
                    Ok(ConnectionEvent::SimplePairingComplete { status, addr })
                }
                EventCode::LinkSupervisionTimeoutChanged => {
                    let handle: u16 = data.read_le()?;
                    let timeout: u16 = data.read_le()?;
                    let timeout = (timeout > 0)
                        .then_some(BASE_BAND_SLOT * timeout as u32);
                    data.finish()?;
                    Ok(ConnectionEvent::LinkSuperVisionTimeoutChanged { handle, timeout })
                }
                EventCode::UserPasskeyNotification => {
                    let addr: RemoteAddr = data.read_le()?;
                    let passkey: u32 = data.read_le()?;
                    ensure!(passkey <= 999999, instructor::Error::InvalidValue);
                    data.finish()?;
                    Ok(ConnectionEvent::UserPasskeyNotification { addr, passkey })
                }
                EventCode::UserPasskeyRequest => {
                    let addr: RemoteAddr = data.read_le()?;
                    data.finish()?;
                    Ok(ConnectionEvent::UserPasskeyRequest { addr })
                }
                EventCode::KeypressNotification => {
                    let addr: RemoteAddr = data.read_le()?;
                    let ty: KeypressNotificationType = data.read_le()?;
                    data.finish()?;
                    Ok(ConnectionEvent::KeypressNotification { addr, ty })
                }
                EventCode::RemoteOobDataRequest => {
                    let addr: RemoteAddr = data.read_le()?;
                    data.finish()?;
                    Ok(ConnectionEvent::RemoteOobDataRequest { addr })
                }
                _ => unreachable!()
            });
            match event {
                Ok(event) => return Poll::Ready(Some(event)),
                Err(err) => warn!("Error parsing connection event {:?}: {:?}", code, err)
            }
        }
        Poll::Pending
    }
}