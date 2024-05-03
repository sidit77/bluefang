use std::collections::{BTreeMap, BTreeSet};
use std::future::pending;
use std::mem::size_of;
use nusb::transfer::{ControlOut, ControlType, Recipient, RequestBuffer, TransferError};
use tokio::sync::mpsc::{UnboundedSender as MpscSender, UnboundedReceiver as MpscReceiver};
use tokio::sync::oneshot::Sender as OneshotSender;
use tracing::{debug, error, trace, warn};
use crate::ensure;
use crate::hci::buffer::{ReceiveBuffer, SendBuffer};
use crate::hci::{Error, Opcode};
use crate::hci::acl::AclDataPacket;
use crate::hci::btsnoop::{LogWriter, PacketType};
use crate::hci::consts::{EventCode, Status};
use crate::host::usb::UsbHost;
use crate::utils::{DispatchExt, SliceExt};

const MAX_HCI_EVENT_SIZE: usize = 1 + size_of::<u8>() + u8::MAX as usize;
const HCI_EVENT_QUEUE_SIZE: usize = 4;

pub enum EventLoopCommand {
    Shutdown,
    RegisterHciEventHandler {
        events: BTreeSet<EventCode>,
        handler: MpscSender<Event>,
    },
    RegisterAclDataHandler {
        handler: MpscSender<AclDataPacket>,
    },
    SetMaxInFlightAclPackets(u32),
}

pub async fn event_loop(
    transport: UsbHost,
    mut cmd_receiver: MpscReceiver<(Opcode, SendBuffer, OneshotSender<Result<ReceiveBuffer, TransferError>>)>,
    mut acl_receiver: MpscReceiver<AclDataPacket>,
    mut ctl_receiver: MpscReceiver<EventLoopCommand>,
) {
    let mut events = transport.interface.interrupt_in_queue(transport.endpoints.event);
    for _ in 0..HCI_EVENT_QUEUE_SIZE {
        events.submit(RequestBuffer::new(MAX_HCI_EVENT_SIZE));
    }

    let mut acl_in = transport.interface.bulk_in_queue(transport.endpoints.acl_in);
    for _ in 0..4 {
        acl_in.submit(RequestBuffer::new(2048)); //TODO Check this
    }
    let mut acl_out = transport.interface.bulk_out_queue(transport.endpoints.acl_out);

    let mut state = State::default();
    let log = LogWriter::new("btsnoop.log");

    loop {
        tokio::select! {
            event = events.next_complete() => {
                match event.status {
                    Ok(_) => {
                        log.write(PacketType::Event, &event.data);
                        match state.process_hci_event(&event.data) {
                            Ok(true) => (),
                            Ok(false) => log.write(PacketType::SystemNode, "Unhandled HCI event".as_bytes()),
                            Err(err) => error!("Error processing HCI event: {:?}", err),
                        }
                    },
                    Err(err) => error!("Error reading HCI event: {:?}", err),
                }
                events.submit(RequestBuffer::reuse(event.data, MAX_HCI_EVENT_SIZE));
            },
            data = acl_in.next_complete() => {
                match data.status {
                    Ok(_) => {
                        log.write(PacketType::AclRx, &data.data);
                        state.process_acl_data(&data.data)
                            .unwrap_or_else(|err| error!("Error processing ACL data: {:?}", err));
                    },
                    Err(err) => error!("Error reading HCI event: {:?}", err),
                }
                acl_in.submit(RequestBuffer::reuse(data.data, MAX_HCI_EVENT_SIZE));
            },
            completion = acl_out.next_complete(), if acl_out.pending() > 0 => {
                completion
                    .status
                    .unwrap_or_else(|err| error!("Error writing ACL data: {:?}", err));
            },
            data = acl_receiver.recv(), if state.in_flight < state.max_in_flight => {
                if let Some(data) = data {
                    state.in_flight += 1;
                    let data = data.into_vec();
                    log.write(PacketType::AclTx, &data);
                    acl_out.submit(data);
                } else  {
                    break;
                }
            },
            cmd = cmd_receiver.recv(), if state.outstanding_command.is_none() => {
                if let Some((opcode, req, tx)) = cmd {
                    log.write(PacketType::Command, &req.data());
                    let cmd = transport.interface.control_out(ControlOut {
                        control_type: ControlType::Class,
                        recipient: Recipient::Interface,
                        request: 0x00,
                        value: 0x00,
                        index: transport.endpoints.main_iface.into(),
                        data: req.data(),
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
    outstanding_command: Option<(Opcode, OneshotSender<Result<ReceiveBuffer, TransferError>>)>,
    hci_event_handlers: BTreeMap<EventCode, Vec<MpscSender<Event>>>,
    acl_data_handlers: Vec<MpscSender<AclDataPacket>>,
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

    fn process_hci_event(&mut self, event: &[u8]) -> Result<bool, Error> {
        let mut event = Event::parse(event)?;
        trace!("Received HCI event: {:?}", event.code);
        match event.code {
            EventCode::CommandComplete | EventCode::CommandStatus => {
                // ([Vol 4] Part E, Section 7.7.14).
                // ([Vol 4] Part E, Section 7.7.15).
                if let EventCode::CommandStatus = event.code {
                    event.data.get_mut().rotate_left(size_of::<Status>());
                }
                let _cmd_quota = event.data.u8()?;
                let opcode= event.data.u16().map(Opcode::from)?;
                // trace!("Received CommandComplete for {:?}", opcode);
                match self.outstanding_command.take() {
                    Some((op, tx)) if op == opcode => {
                        tx.send(Ok(event.data))
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
                let count = event.data.u8()? as usize;
                let (handles, counts) = event.data.bytes(count * 4)?.split_at(count * 2);
                for i in 0..count {
                    let handle = handles.get_chunk(i * 2).copied().map(u16::from_le_bytes).unwrap();
                    let count = counts.get_chunk(i * 2).copied().map(u16::from_le_bytes).unwrap();
                    trace!("Flushed {} packets for handle {}", count, handle);
                    self.in_flight = self.in_flight.saturating_sub(count as u32);
                }
                event.data.finish()?;
                Ok(true)
            },
            _ => {
                let code = event.code;
                let handled = self.hci_event_handlers
                    .get_mut(&code)
                    .map_or(false, |handlers| handlers.dispatch(event));
                if !handled {
                    warn!("Unhandled HCI event: {:?}", code);
                }
                Ok(handled)
            },
        }
    }

    fn process_acl_data(&mut self, data: &[u8]) -> Result<(), Error> {
        let data = AclDataPacket::from_bytes(data).ok_or(Error::BadEventPacketSize)?;
        self.acl_data_handlers.dispatch(data);
        Ok(())
    }

}

#[derive(Debug, Clone)]
pub struct Event {
    pub code: EventCode,
    pub data: ReceiveBuffer,
}

impl Event {
    /// HCI event packet ([Vol 4] Part E, Section 5.4.4).
    fn parse(data: &[u8]) -> Result<Self, Error> {
        data
            .split_first_chunk()
            .ok_or(Error::BadEventPacketSize)
            .and_then(|([code, len], payload)| {
                let code = EventCode::try_from(*code)
                    .map_err(|_| Error::UnknownEventCode(*code))?;
                ensure!(*len as usize == payload.len(), Error::BadEventPacketSize);
                Ok(Self {
                    code,
                    data: ReceiveBuffer::from_payload(payload)
                })
            })
    }
}


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