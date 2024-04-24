use std::sync::Arc;
use tokio::spawn;
use tracing::{debug, trace, warn};
use crate::hci::consts::{ClassOfDevice, LinkType, RemoteAddr, Role, Status};
use crate::hci::{Error, Hci};

pub async fn handle_connection(hci: Arc<Hci>) -> Result<(), Error> {
    let mut connection_events = hci.router.connection_events()
        .expect("Another connection handler is already running");
    spawn(async move {
        while let Some(event) = connection_events.recv().await {
            trace!("Connection event: {:?}", event);
            match event {
                ParsedConnectionEvent::ConnectionRequest(addr, _class, _link_type) => {
                    hci.accept_connection_request(addr, Role::Slave)
                        .await
                        .unwrap_or_else(|err| warn!("Error accepting connection request: {:?}", err));
                },
                ParsedConnectionEvent::ConnectionComplete(status, handle, addr, link_type, encryption_enabled) => {
                    if status == Status::Success {
                        debug!("Connection complete: {:?} {:?} {:?} {:?}", handle, addr, link_type, encryption_enabled);
                    } else {
                        warn!("Connection failed: {:?}", status);
                    }
                },
                ParsedConnectionEvent::PinCodeRequest(addr) => {
                    hci.pin_code_request_reply(addr, "0000")
                        .await
                        .map(|_| ())
                        .unwrap_or_else(|err| warn!("Error replying to pin code request: {:?}", err));
                }
                ParsedConnectionEvent::LinkKeyNotification(addr, key, key_type) => {
                    debug!("Link key notification: {} {:X?} 0x{:X}", addr, key, key_type);
                }
                ParsedConnectionEvent::DisconnectionComplete(status, conn, reason) => {
                    if status == Status::Success {
                        debug!("Disconnection complete: {:?} {:?}", conn, reason);
                    } else {
                        warn!("Disconnection failed: {:?}", status);
                    }
                }
            }
        }
    });
    Ok(())
}

#[derive(Debug)]
pub enum ParsedConnectionEvent {
    ConnectionRequest(RemoteAddr, ClassOfDevice, LinkType),
    ConnectionComplete(Status, u16, RemoteAddr, LinkType, bool),
    PinCodeRequest(RemoteAddr),
    LinkKeyNotification(RemoteAddr, [u8; 16], u8),
    DisconnectionComplete(Status, u16, Status),
}
