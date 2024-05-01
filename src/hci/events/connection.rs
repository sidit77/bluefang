use tokio::sync::mpsc::error::TrySendError;
use tracing::{warn};
use crate::hci::buffer::ReceiveBuffer;
use crate::hci::consts::{ClassOfDevice, LinkType, RemoteAddr, Status};
use crate::hci::Error;
use crate::hci::events::{ConnectionEvent, EventRouter};

impl EventRouter {

    pub fn handle_connection_events(&self, event: ConnectionEvent, mut payload: ReceiveBuffer) -> Result<(), Error> {
        let mut manager = self.connection_manager.lock();
        let remove = match manager.as_ref() {
            Some(manager) => {
                let parsed = match event {
                    ConnectionEvent::ConnectionComplete => {
                        // ([Vol 4] Part E, Section 7.7.3).
                        let status = payload.u8().map(Status::from)?;
                        let handle = payload.u16()?;
                        let addr = payload.bytes().map(RemoteAddr::from)?;
                        let link_type = payload.u8().map(LinkType::from)?;
                        let encryption_enabled = payload.u8().map(|b| b == 0x01)?;
                        payload.finish()?;
                        ParsedConnectionEvent::ConnectionComplete(status, handle, addr, link_type, encryption_enabled)
                    }
                    ConnectionEvent::ConnectionRequest => {
                        // ([Vol 4] Part E, Section 7.7.4).
                        let addr = payload.bytes().map(RemoteAddr::from)?;
                        let class = payload.u24().map(ClassOfDevice::from)?;
                        let link_type = payload.u8().map(LinkType::from)?;
                        payload.finish()?;
                        ParsedConnectionEvent::ConnectionRequest(addr, class, link_type)
                    }
                    ConnectionEvent::PinCodeRequest => {
                        // ([Vol 4] Part E, Section 7.7.22).
                        let addr = payload.bytes().map(RemoteAddr::from)?;
                        payload.finish()?;
                        ParsedConnectionEvent::PinCodeRequest(addr)
                    },
                    ConnectionEvent::LinkKeyNotification => {
                        // ([Vol 4] Part E, Section 7.7.24).
                        let addr = payload.bytes().map(RemoteAddr::from)?;
                        let key = payload.bytes::<16>()?;
                        let key_type = payload.u8()?;
                        payload.finish()?;
                        ParsedConnectionEvent::LinkKeyNotification(addr, key, key_type)
                    },
                    ConnectionEvent::DisconnectionComplete => {
                        // ([Vol 4] Part E, Section 7.7.5).
                        let status = payload.u8().map(Status::from)?;
                        let handle = payload.u16()?;
                        let reason = payload.u8().map(Status::from)?;
                        payload.finish()?;
                        ParsedConnectionEvent::DisconnectionComplete(status, handle, reason)
                    }
                };
                match manager.try_send(parsed) {
                    Ok(_) => false,
                    Err(TrySendError::Closed(_)) => true,
                    Err(TrySendError::Full(event)) => {
                        warn!("Connection event queue full. Discarding event: {:?}", event);
                        false
                    },
                }
            },
            None => false,
        };
        if remove {
            *manager = None;
        }
        Ok(())
    }

}