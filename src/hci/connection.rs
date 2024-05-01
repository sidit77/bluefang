use std::sync::Arc;
use tokio::spawn;
use tokio::sync::mpsc::unbounded_channel;
use tracing::{debug, trace, warn};
use crate::ensure;
use crate::hci::consts::{ClassOfDevice, EventCode, LinkType, RemoteAddr, Role, Status};
use crate::hci::{Error, Hci};
use crate::hci::acl::AclDataAssembler;
use crate::hci::event_loop::Event;

pub fn handle_connection(hci: Arc<Hci>) -> Result<(), Error> {
    let mut events = {
        let (tx, rx) = unbounded_channel();
        hci.register_event_handler(
            [
                EventCode::ConnectionRequest,
                EventCode::ConnectionComplete,
                EventCode::DisconnectionComplete,
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

async fn handle_event(hci: &Hci, event: Event) -> Result<(), Error> {
    let Event { code, mut data, .. } = event;
    match code {
        EventCode::ConnectionRequest => {
            // ([Vol 4] Part E, Section 7.7.4).
            let addr = data.bytes().map(RemoteAddr::from)?;
            let _class = data.u24().map(ClassOfDevice::from)?;
            let link_type = data.u8().map(LinkType::from)?;
            data.finish()?;

            ensure!(link_type == LinkType::Acl, "Invalid link type");
            hci.accept_connection_request(addr, Role::Slave).await?;
        },
        EventCode::ConnectionComplete => {
            // ([Vol 4] Part E, Section 7.7.3).
            let status = data.u8().map(Status::from)?;
            let handle = data.u16()?;
            let addr = data.bytes().map(RemoteAddr::from)?;
            let link_type = data.u8().map(LinkType::from)?;
            let _encryption_enabled = data.u8().map(|b| b == 0x01)?;
            data.finish()?;

            assert_eq!(link_type, LinkType::Acl);
            if status == Status::Success {
                //assert!(CONNECTIONS
                //    .lock()
                //    .insert(handle, PhysicalConnection {
                //        handle,
                //        addr,
                //        assembler: AclDataAssembler::default(),
                //    }).is_none());
                debug!("Connection complete: 0x{:04X} {}", handle, addr);
            } else {
                warn!("Connection failed: {:?}", status);
            }
        },
        EventCode::DisconnectionComplete => {
            // ([Vol 4] Part E, Section 7.7.5).
            let status = data.u8().map(Status::from)?;
            let handle = data.u16()?;
            let reason = data.u8().map(Status::from)?;
            data.finish()?;

            //CONNECTIONS.lock().remove(&conn);
            if status == Status::Success {
                debug!("Disconnection complete: {:?} {:?}", handle, reason);
            } else {
                warn!("Disconnection failed: {:?}", status);
            }
        },
        EventCode::PinCodeRequest => {
            // ([Vol 4] Part E, Section 7.7.22).
            let addr = data.bytes().map(RemoteAddr::from)?;
            data.finish()?;

            debug!("Pin code request: {}", addr);
            hci.pin_code_request_reply(addr, "0000").await?;
        },
        EventCode::LinkKeyNotification => {
            // ([Vol 4] Part E, Section 7.7.24).
            let addr = data.bytes().map(RemoteAddr::from)?;
            let key = data.bytes::<16>()?;
            let key_type = data.u8()?;
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
