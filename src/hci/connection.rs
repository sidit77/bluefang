use std::collections::BTreeMap;
use std::sync::Arc;
use parking_lot::Mutex;
use tokio::spawn;
use tracing::{debug, trace, warn};
use crate::ensure;
use crate::hci::consts::{ClassOfDevice, LinkType, RemoteAddr, Role, Status};
use crate::hci::{Error, Hci};
use crate::hci::acl::AclDataAssembler;

pub async fn handle_connection(hci: Arc<Hci>) -> Result<(), Error> {
    let mut connection_events = hci.router.connection_events()
        .expect("Another connection handler is already running");
    spawn(async move {
        while let Some(event) = connection_events.recv().await {
            trace!("Connection event: {:?}", event);
            handle_event(&hci, event).await.unwrap_or_else(|err| {
                warn!("Error handling connection event: {:?}", err);
            });
        }
    });
    Ok(())
}

pub static CONNECTIONS: Mutex<BTreeMap<u16, PhysicalConnection>> = Mutex::new(BTreeMap::new());

async fn handle_event(hci: &Hci, event: ParsedConnectionEvent) -> Result<(), Error> {
    match event {
        ParsedConnectionEvent::ConnectionRequest(addr, _, link_type) => {
            ensure!(link_type == LinkType::Acl, "Invalid link type");
            hci.accept_connection_request(addr, Role::Slave).await?;
        }
        ParsedConnectionEvent::ConnectionComplete(status, handle, addr, link_type, _) => {
            assert_eq!(link_type, LinkType::Acl);
            if status == Status::Success {
                assert!(CONNECTIONS
                    .lock()
                    .insert(handle, PhysicalConnection {
                        handle,
                        addr,
                        assembler: AclDataAssembler::default(),
                    }).is_none());
                debug!("Connection complete: 0x{:04X} {}", handle, addr);

            } else {
                warn!("Connection failed: {:?}", status);
            }
        }
        ParsedConnectionEvent::PinCodeRequest(addr) => {
            hci.pin_code_request_reply(addr, "0000").await?;
        }
        ParsedConnectionEvent::LinkKeyNotification(addr, key, key_type) => {
            debug!("Link key notification: {} {:X?} 0x{:X}", addr, key, key_type);
        }
        ParsedConnectionEvent::DisconnectionComplete(status, conn, reason) => {
            CONNECTIONS.lock().remove(&conn);
            if status == Status::Success {
                debug!("Disconnection complete: {:?} {:?}", conn, reason);
            } else {
                warn!("Disconnection failed: {:?}", status);
            }
        }
    }
    Ok(())
}


pub struct PhysicalConnection {
    pub handle: u16,
    pub addr: RemoteAddr,
    pub assembler: AclDataAssembler
}

#[derive(Debug)]
pub enum ParsedConnectionEvent {
    ConnectionRequest(RemoteAddr, ClassOfDevice, LinkType),
    ConnectionComplete(Status, u16, RemoteAddr, LinkType, bool),
    PinCodeRequest(RemoteAddr),
    LinkKeyNotification(RemoteAddr, [u8; 16], u8),
    DisconnectionComplete(Status, u16, Status),
}
