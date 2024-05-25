use std::collections::{BTreeMap, BTreeSet};
use std::future::pending;
use std::mem::size_of;
use bytes::{BufMut, Bytes, BytesMut};
use instructor::{Buffer, Exstruct};
use instructor::utils::Length;
use nusb::transfer::{ControlOut, ControlType, Recipient, RequestBuffer, TransferError};
use tokio::sync::mpsc::{UnboundedSender as MpscSender, UnboundedReceiver as MpscReceiver};
use tokio::sync::oneshot::Sender as OneshotSender;
use tracing::{debug, error, trace, warn};
use crate::hci::{Error, Opcode};
use crate::hci::btsnoop::{LogWriter, PacketType};
use crate::hci::consts::{EventCode, Status};
use crate::host::usb::UsbHost;
use crate::utils::{DispatchExt};

const TRANSFER_BUFFER_SIZE: usize = 4096;
const TRANSFER_BUFFER_COUNT: usize = 4;

pub enum EventLoopCommand {
    Shutdown,
    RegisterHciEventHandler {
        events: BTreeSet<EventCode>,
        handler: MpscSender<(EventCode, Bytes)>,
    },
    RegisterAclDataHandler {
        handler: MpscSender<Bytes>,
    },
    SetMaxInFlightAclPackets(u32),
}

pub async fn event_loop(
    transport: UsbHost,
    mut cmd_receiver: MpscReceiver<(Opcode, Bytes, OneshotSender<Result<Bytes, TransferError>>)>,
    mut acl_receiver: MpscReceiver<Bytes>,
    mut ctl_receiver: MpscReceiver<EventLoopCommand>,
) {

    let mut events = transport.interface.interrupt_in_queue(transport.endpoints.event);
    for _ in 0..TRANSFER_BUFFER_COUNT {
        events.submit(RequestBuffer::new(TRANSFER_BUFFER_SIZE));
    }

    let mut acl_in = transport.interface.bulk_in_queue(transport.endpoints.acl_in);
    for _ in 0..TRANSFER_BUFFER_COUNT {
        acl_in.submit(RequestBuffer::new(TRANSFER_BUFFER_SIZE));
    }
    let mut acl_out = transport.interface.bulk_out_queue(transport.endpoints.acl_out);

    let mut state = State::default();
    let log = LogWriter::new("btlog.snoop");
    let mut buffer = BytesMut::with_capacity(4096);

    loop {
        tokio::select! {
            event = events.next_complete() => {
                match event.status {
                    Ok(_) => {
                        buffer.put_slice(&event.data);
                        let data = buffer.split().freeze();
                        log.write(PacketType::Event, data.clone());
                        match state.process_hci_event(data) {
                            Ok(true) => (),
                            Ok(false) => log.write(PacketType::SystemNode, Bytes::from_static("Unhandled HCI event".as_bytes())),
                            Err(err) => error!("Error processing HCI event: {:?}", err),
                        }
                    },
                    Err(err) => error!("Error reading HCI event: {:?}", err),
                }
                events.submit(RequestBuffer::reuse(event.data, TRANSFER_BUFFER_SIZE));
            },
            data = acl_in.next_complete() => {
                match data.status {
                    Ok(_) => {
                        buffer.put_slice(&data.data);
                        let data = buffer.split().freeze();
                        log.write(PacketType::AclRx, data.clone());
                        state.process_acl_data(data)
                            .unwrap_or_else(|err| error!("Error processing ACL data: {:?}", err));
                    },
                    Err(err) => error!("Error reading HCI event: {:?}", err),
                }
                acl_in.submit(RequestBuffer::reuse(data.data, TRANSFER_BUFFER_SIZE));
            },
            completion = acl_out.next_complete(), if acl_out.pending() > 0 => {
                completion
                    .status
                    .unwrap_or_else(|err| error!("Error writing ACL data: {:?}", err));
            },
            data = acl_receiver.recv(), if state.in_flight < state.max_in_flight => {
                if let Some(data) = data {
                    state.in_flight += 1;
                    log.write(PacketType::AclTx, data.clone());
                    let data = data.to_vec();
                    acl_out.submit(data);
                } else  {
                    break;
                }
            },
            cmd = cmd_receiver.recv(), if state.outstanding_command.is_none() => {
                if let Some((opcode, req, tx)) = cmd {
                    log.write(PacketType::Command, req.clone());
                    let cmd = transport.interface.control_out(ControlOut {
                        control_type: ControlType::Class,
                        recipient: Recipient::Interface,
                        request: 0x00,
                        value: 0x00,
                        index: transport.endpoints.main_iface.into(),
                        data: &req,
                    }).await;
                    match cmd.status {
                        Ok(_) => state.outstanding_command = Some((opcode, tx)),
                        Err(err) => {
                            let _ = tx.send(Err(err.into()));
                        }
                    }
                } else {
                    break;
                }
            },
            _ = state.outstanding_command_dropped() => {
                state.outstanding_command = None;
            },
            cmd = ctl_receiver.recv() => {
                match cmd {
                    Some(EventLoopCommand::RegisterHciEventHandler { events, handler }) => {
                        for event in events {
                            state.hci_event_handlers.entry(event).or_default().push(handler.clone());
                        }
                    }
                    Some(EventLoopCommand::RegisterAclDataHandler { handler }) => {
                        state.acl_data_handlers.push(handler);
                    }
                    Some(EventLoopCommand::SetMaxInFlightAclPackets(n)) => {
                        state.max_in_flight = n;
                    }
                    Some(EventLoopCommand::Shutdown) | None => {
                        break;
                    }
                }
            }
        }
    }

    debug!("Event loop closed");
}

#[derive(Default)]
struct State {
    outstanding_command: Option<(Opcode, OneshotSender<Result<Bytes, TransferError>>)>,
    hci_event_handlers: BTreeMap<EventCode, Vec<MpscSender<(EventCode, Bytes)>>>,
    acl_data_handlers: Vec<MpscSender<Bytes>>,
    max_in_flight: u32,
    in_flight: u32,
}

impl State {

    async fn outstanding_command_dropped(&mut self) {
        match self.outstanding_command.as_mut() {
            None => pending().await,
            Some((_, tx)) => tx.closed().await
        }
    }

    fn process_hci_event(&mut self, mut data: Bytes) -> Result<bool, Error> {
        let header: EventHeader = data.read_le()?;
        //trace!("Received HCI event: {:?}", header.code);
        match header.code {
            EventCode::CommandComplete | EventCode::CommandStatus => {
                // ([Vol 4] Part E, Section 7.7.14).
                // ([Vol 4] Part E, Section 7.7.15).
                if let EventCode::CommandStatus = header.code {
                    let mut tmp = BytesMut::with_capacity(data.len());
                    tmp.put(data);
                    tmp.rotate_left(size_of::<Status>());
                    data = tmp.freeze();
                }
                let _cmd_quota: u8 = data.read_le()?;
                let opcode: Opcode = data.read_le()?;
                // trace!("Received CommandComplete for {:?}", opcode);
                match self.outstanding_command.take() {
                    Some((op, tx)) if op == opcode => {
                        tx.send(Ok(data))
                            .unwrap_or_else(|_| debug!("CommandComplete receiver dropped"))
                    },
                    Some((op, tx)) => {
                        self.outstanding_command = Some((op, tx));
                        return Err(Error::UnexpectedCommandResponse(opcode));
                    },
                    None => return Err(Error::UnexpectedCommandResponse(opcode))
                }
                Ok(true)
            },
            EventCode::NumberOfCompletedPackets => {
                // ([Vol 4] Part E, Section 7.7.19).
                let count = data.read_le::<u8>()? as usize;
                let mut handles = data.split_to(count * 2);
                let mut counts = data.split_to(count * 2);
                for _ in 0..count {
                    let handle: u16 = handles.read_le()?;
                    let count: u16 = counts.read_le()?;
                    trace!("Flushed {} packets for handle {}", count, handle);
                    self.in_flight = self.in_flight.saturating_sub(count as u32);
                }
                data.finish()?;
                Ok(true)
            },
            _ => {
                let code = header.code;
                let handled = self.hci_event_handlers
                    .get_mut(&code)
                    .map_or(false, |handlers| handlers.dispatch((code, data)));
                if !handled {
                    warn!("Unhandled HCI event: {:?}", code);
                }
                Ok(handled)
            },
        }
    }

    fn process_acl_data(&mut self, data: Bytes) -> Result<(), Error> {
        // let data = AclDataPacket::from_bytes(data).ok_or(Error::BadEventPacketSize)?;
        self.acl_data_handlers.dispatch(data);
        Ok(())
    }

}

/// HCI event packet ([Vol 4] Part E, Section 5.4.4).
#[derive(Debug, Clone, Exstruct)]
pub struct EventHeader {
    pub code: EventCode,
    pub length: Length<u8, 0>
}

//impl Event {
//
//    fn parse(data: &[u8]) -> Result<Self, Error> {
//        data
//            .split_first_chunk()
//            .ok_or(Error::BadEventPacketSize)
//            .and_then(|([code, len], payload)| {
//                let code = EventCode::try_from(*code)
//                    .map_err(|_| Error::UnknownEventCode(*code))?;
//                ensure!(*len as usize == payload.len(), Error::BadEventPacketSize);
//                Ok(Self {
//                    code,
//                    data: ReceiveBuffer::from_payload(payload)
//                })
//            })
//    }
//}


/*
InquiryEvent::Complete => {
    // ([Vol 4] Part E, Section 7.7.1).
    let status = Status::from(payload.u8()?);
    payload.finish()?;
    debug!("Inquiry complete: {}", status);
},
InquiryEvent::Result => {
    // ([Vol 4] Part E, Section 7.7.2).
    let count = payload.u8()? as usize;
    let addr: SmallVec<[RemoteAddr; 2]> = (0..count)
        .map(|_| payload.bytes().map(RemoteAddr::from))
        .collect::<Result<_, _>>()?;
    payload.skip(count * 3); // repetition mode
    let classes: SmallVec<[ClassOfDevice; 2]> = (0..count)
        .map(|_| payload
            .u24()
            .map(ClassOfDevice::from))
        .collect::<Result<_, _>>()?;
    payload.skip(count * 2); // clock offset
    payload.finish()?;

    for i in 0..count {
        debug!("Inquiry result: {} {:?}", addr[i], classes[i]);
    }
}
 */