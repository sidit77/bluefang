use std::sync::Arc;
use bytes::Bytes;
use instructor::Buffer;
use tokio::spawn;
use tokio::sync::mpsc::unbounded_channel;
use tracing::{debug, trace, warn};
use crate::ensure;
use crate::hci::consts::{ClassOfDevice, EventCode, LinkType, RemoteAddr, Role, Status};
use crate::hci::{Error, Hci};
use crate::hci::acl::AclDataAssembler;

pub fn handle_connection(hci: Arc<Hci>) -> Result<(), Error> {
    let mut events = {
        let (tx, rx) = unbounded_channel();
        hci.register_event_handler(
            [
                EventCode::ConnectionRequest,
                // EventCode::ConnectionComplete,
                // EventCode::DisconnectionComplete,
                EventCode::PinCodeRequest,
                EventCode::LinkKeyNotification,
            ],
            tx)?;
        rx
    };
    spawn(async move {
        while let Some(event) = events.recv().await {
            // trace!("Connection event: {:?}", event);
            handle_event(&hci, event).await.unwrap_or_else(|err| {
                warn!("Error handling connection event: {:?}", err);
            });
        }
        trace!("Connection event handler finished");
    });
    Ok(())
}

async fn handle_event(hci: &Hci, (code, mut data): (EventCode, Bytes)) -> Result<(), Error> {
    match code {
        EventCode::ConnectionRequest => {
            // ([Vol 4] Part E, Section 7.7.4).
            let addr: RemoteAddr = data.read_le()?;
            let _class: ClassOfDevice = data.read_le()?;
            let link_type: LinkType = data.read_le()?;
            data.finish()?;

            ensure!(link_type == LinkType::Acl, "Invalid link type");
            hci.accept_connection_request(addr, Role::Slave).await?;
        },
        EventCode::PinCodeRequest => {
            // ([Vol 4] Part E, Section 7.7.22).
            let addr: RemoteAddr = data.read_le()?;
            data.finish()?;

            debug!("Pin code request: {}", addr);
            hci.pin_code_request_reply(addr, "0000").await?;
        },
        EventCode::LinkKeyNotification => {
            // ([Vol 4] Part E, Section 7.7.24).
            let addr: RemoteAddr = data.read_le()?;
            let key: [u8; 16] = data.read_le()?;
            let key_type: u8 = data.read_le()?;
            data.finish()?;

            debug!("Link key notification: {} {:X?} 0x{:X}", addr, key, key_type);
        },
        _ => unreachable!()
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
