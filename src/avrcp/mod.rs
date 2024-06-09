use std::collections::BTreeSet;
use std::sync::Arc;
use bytes::{Bytes, BytesMut};
use instructor::{BigEndian, Buffer, BufferMut, Exstruct, Instruct};
use instructor::utils::u24;
use parking_lot::Mutex;
use tokio::{spawn};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::oneshot::{Sender as OneshotSender};
use tracing::{error, info, trace, warn};
use crate::avc::{CommandCode, Frame, Opcode, PassThroughFrame, Subunit, SubunitType};
use crate::avctp::{Avctp, Message, MessageType};
use crate::avrcp::sdp::REMOTE_CONTROL_SERVICE;
use crate::{ensure, hci, log_assert};
use crate::avrcp::error::{AvcError, ErrorCode};
use crate::avrcp::packets::{BLUETOOTH_SIG_COMPANY_ID, Command, CommandAssembler, COMPANY_ID_CAPABILITY, EVENTS_SUPPORTED_CAPABILITY, fragment_command, PANEL, Pdu};
use crate::l2cap::channel::Channel;
use crate::l2cap::{AVCTP_PSM, ProtocolDelegate, ProtocolHandler, ProtocolHandlerProvider};
use crate::utils::{Either2, select2};
use crate::avrcp::session::{EventParser, AvrcpCommand};

pub mod sdp;
mod packets;
mod error;
mod session;

pub use session::{AvrcpSession, SessionError, Event, Notification, notifications};
pub use packets::{EventId, MediaAttributeId};



#[derive(Default)]
pub struct AvrcpBuilder;

impl AvrcpBuilder {
    pub fn build<F: FnMut(AvrcpSession) + Send + 'static>(self, handler: F) -> Avrcp {
        Avrcp {
            existing_connections: Arc::new(Mutex::new(BTreeSet::new())),
            session_handler: Arc::new(Mutex::new(handler)),
        }
    }
}

#[derive(Clone)]
pub struct Avrcp {
    existing_connections: Arc<Mutex<BTreeSet<u16>>>,
    session_handler: Arc<Mutex<dyn FnMut(AvrcpSession) + Send>>
}

impl ProtocolHandlerProvider for Avrcp {
    fn protocol_handlers(&self) -> Vec<Box<dyn ProtocolHandler>> {
        vec![
            ProtocolDelegate::new(AVCTP_PSM, self.clone(), Self::handle_control)
        ]
    }
}

impl Avrcp {
    pub fn handle_control(&self, mut channel: Channel) {
        let handle = channel.connection_handle;
        let success = self.existing_connections.lock().insert(handle);
        if success {
            let existing_connections = self.existing_connections.clone();
            let session_handler = self.session_handler.clone();
            spawn(async move {
                if let Err(err) = channel.configure().await {
                    warn!("Error configuring channel: {:?}", err);
                    return;
                }
                let (cmd_tx, cmd_rx) = tokio::sync::mpsc::channel(16);
                let (evt_tx, evt_rx) = tokio::sync::mpsc::channel(16);
                let mut state = State {
                    avctp: Avctp::new(channel, [REMOTE_CONTROL_SERVICE]),
                    command_assembler: Default::default(),
                    response_assembler: Default::default(),
                    volume: Volume(1.0),
                    commands: cmd_rx,
                    events: evt_tx,
                    outstanding_transactions: Default::default(),
                };
                session_handler.lock()(AvrcpSession { commands: cmd_tx, events: evt_rx });
                state.run().await.unwrap_or_else(|err| {
                    warn!("Error running avctp: {:?}", err);
                });
                trace!("AVCTP connection closed");
                existing_connections.lock().remove(&handle);
            });
        }
    }
}

#[derive(Default, Debug)]
enum TransactionState {
    #[default]
    Empty,
    PendingPassThrough(OneshotSender<Result<Bytes, SessionError>>),
    PendingVendorDependent(CommandCode, OneshotSender<Result<Bytes, SessionError>>),
    PendingNotificationRegistration(EventParser, OneshotSender<Result<Bytes, SessionError>>),
    WaitingForChange(EventParser)
}

impl TransactionState {
    pub fn is_free(&self) -> bool {
        matches!(self, TransactionState::Empty)
    }

    pub fn take_sender(&mut self) -> OneshotSender<Result<Bytes, SessionError>> {
        let prev = std::mem::take(self);
        match prev {
            TransactionState::PendingPassThrough(sender) => sender,
            TransactionState::PendingVendorDependent(_, sender) => sender,
            TransactionState::PendingNotificationRegistration(parser, sender) => {
                *self = TransactionState::WaitingForChange(parser);
                sender
            },
            _ => unreachable!()
        }
    }

}

struct State {
    avctp: Avctp,
    command_assembler: CommandAssembler,
    response_assembler: CommandAssembler,

    volume: Volume,

    commands: Receiver<(AvrcpCommand, OneshotSender<Result<Bytes, SessionError>>)>,
    events: Sender<Event>,
    outstanding_transactions: [TransactionState; 16]
}

impl State {
    async fn run(&mut self) -> Result<(), hci::Error> {
        loop {
            match select2(self.avctp.read(), self.commands.recv()).await {
                Either2::A(Some(mut packet)) => {
                    let transaction_label = packet.transaction_label;
                    if let Ok(frame) = packet.data.read_be::<Frame>() {
                        let payload = packet.data.clone();
                        if let Err(AvcError::NotImplemented) = self.process_message(frame, packet) {
                            if !frame.ctype.is_response() {
                                self.send_avc(transaction_label, Frame { ctype: CommandCode::NotImplemented, ..frame }, payload);
                            } else {
                                warn!("Failed to handle response: {:?}", frame);
                            }
                        }
                    }
                },
                Either2::B(Some((cmd, sender))) => {
                    let Some(transaction) = self
                        .outstanding_transactions
                        .iter()
                        .position(|x| x.is_free())
                        else {
                            let _ = sender.send(Err(SessionError::NoTransactionIdAvailable));
                            continue;
                        };
                    match cmd {
                        AvrcpCommand::PassThrough(op, state) => {
                            self.send_avc(
                                transaction as u8,
                                Frame {
                                    ctype: CommandCode::Control,
                                    subunit: PANEL,
                                    opcode: Opcode::PassThrough,
                                },
                                PassThroughFrame { op, state, data_len: 0 })
                                .then(|| self.outstanding_transactions[transaction] = TransactionState::PendingPassThrough(sender));
                        },
                        AvrcpCommand::VendorSpecific(cmd, pdu, params) => {
                            // These should be registered using register notification
                            debug_assert!(cmd != CommandCode::Notify);
                            self.send_avrcp(
                                transaction as u8,
                                cmd,
                                pdu,
                                params)
                                .then(|| self.outstanding_transactions[transaction] = TransactionState::PendingVendorDependent(cmd, sender));
                        }
                        AvrcpCommand::RegisterNotification(event, parser) => {
                            self.send_avrcp(
                                transaction as u8,
                                CommandCode::Notify,
                                Pdu::RegisterNotification,
                                (event, 0u32))
                                .then(|| self.outstanding_transactions[transaction] = TransactionState::PendingNotificationRegistration(parser, sender));
                        }
                    }
                },
                _ => break
            }
        }
        Ok(())
    }

    fn process_message(&mut self, frame: Frame, mut message: Message) -> Result<(), AvcError> {
        match frame.opcode {
            Opcode::VendorDependent => {
                ensure!(frame.subunit == PANEL, AvcError::NotImplemented, "Unsupported subunit: {:?}", frame.subunit);
                let company_id: u24 = message.data.read_be::<u24>()?;
                ensure!(company_id == BLUETOOTH_SIG_COMPANY_ID, AvcError::NotImplemented, "Unsupported company id: {:#06x}", company_id);
                if frame.ctype.is_response() {
                    if let Some(Command { pdu, mut parameters }) = self.response_assembler.process_msg(message.data)? {
                        let transaction = &mut self.outstanding_transactions[message.transaction_label as usize];
                        match transaction {
                            TransactionState::PendingVendorDependent(CommandCode::Control, _) => {
                                let reply = match frame.ctype {
                                    CommandCode::NotImplemented => Err(SessionError::NotImplemented),
                                    CommandCode::Accepted => Ok(parameters),
                                    CommandCode::Rejected => Err(SessionError::Rejected),
                                    CommandCode::Interim => return Ok(()),
                                    _ => Err(SessionError::InvalidReturnData)
                                };
                                let _ = transaction.take_sender().send(reply);
                            },
                            TransactionState::PendingVendorDependent(CommandCode::Status, _) => {
                                let reply = match frame.ctype {
                                    CommandCode::NotImplemented => Err(SessionError::NotImplemented),
                                    CommandCode::Implemented => Ok(parameters),
                                    CommandCode::Rejected => Err(SessionError::Rejected),
                                    CommandCode::InTransition => Err(SessionError::Busy),
                                    _ => Err(SessionError::InvalidReturnData)
                                };
                                let _ = transaction.take_sender().send(reply);
                            },
                            TransactionState::PendingVendorDependent(code, _) => {
                                error!("Received response for invalid command code: {:?}", code);
                                *transaction = TransactionState::Empty;
                            },
                            TransactionState::PendingNotificationRegistration(_, _) => {
                                let reply = match frame.ctype {
                                    CommandCode::NotImplemented => Err(SessionError::NotImplemented),
                                    CommandCode::Rejected => Err(SessionError::Rejected),
                                    CommandCode::Interim => Ok(parameters),
                                    CommandCode::Changed => {
                                        warn!("Received changed response without interims response");
                                        Err(SessionError::InvalidReturnData)
                                    },
                                    _ => Err(SessionError::InvalidReturnData)
                                };
                                let _ = transaction.take_sender().send(reply);
                            },
                            TransactionState::WaitingForChange(parser) => {
                                log_assert!(frame.ctype == CommandCode::Changed);
                                let event = parameters
                                    .read_be::<EventId>()
                                    .and_then(|_| parser(&mut parameters))
                                    .map_err(|err| {
                                        error!("Error parsing event: {:?}", err);
                                    });
                                if let Ok(event) = event {
                                    self.events.try_send(event).unwrap_or_else(|err| {
                                        warn!("Error sending event: {:?}", err);
                                    });
                                }
                                *transaction = TransactionState::Empty;
                            }
                            _ => {
                                warn!("Received vendor dependent response with no/wrong outstanding transaction: {:?} {:?} {:?}", transaction, pdu, frame.ctype);
                                return Ok(());
                            }
                        }
                    } else {
                        //TODO send continue and continue abort responses
                    }
                } else {
                    if let Some(Command { pdu, parameters }) = self.command_assembler.process_msg(message.data)? {
                        if let Err(err) = self.process_command(message.transaction_label, frame.ctype, pdu, parameters) {
                            self.send_avrcp(message.transaction_label, CommandCode::Rejected, pdu, err);
                        }
                    }
                }

                Ok(())
            },
            Opcode::UnitInfo => {
                const UNIT_INFO: Subunit = Subunit {ty: SubunitType::Unit, id: 7};
                ensure!(frame.ctype == CommandCode::Status, AvcError::NotImplemented, "Unsupported command type: {:?}", frame.ctype);
                ensure!(frame.subunit == UNIT_INFO, AvcError::NotImplemented, "Unsupported subunit: {:?}", frame.subunit);
                self.send_avc(message.transaction_label, Frame {
                    ctype: CommandCode::Implemented,
                    subunit: UNIT_INFO,
                    opcode: Opcode::UnitInfo,
                }, (7u8, PANEL, BLUETOOTH_SIG_COMPANY_ID));
                Ok(())
            },
            Opcode::SubunitInfo => {
                const UNIT_INFO: Subunit = Subunit {ty: SubunitType::Unit, id: 7};
                ensure!(frame.ctype == CommandCode::Status, AvcError::NotImplemented, "Unsupported command type: {:?}", frame.ctype);
                ensure!(frame.subunit == UNIT_INFO, AvcError::NotImplemented, "Unsupported subunit: {:?}", frame.subunit);
                let page: u8 = message.data.read_be()?;
                self.send_avc(message.transaction_label, Frame {
                    ctype: CommandCode::Implemented,
                    subunit: UNIT_INFO,
                    opcode: Opcode::SubunitInfo,
                }, (page, PANEL, [0xffu8; 3]));
                Ok(())
            }
            Opcode::PassThrough => {
                ensure!(frame.subunit == PANEL, AvcError::NotImplemented, "Unsupported subunit: {:?}", frame.subunit);
                let transaction = &mut self.outstanding_transactions[message.transaction_label as usize];
                if !matches!(transaction, TransactionState::PendingPassThrough(_)) {
                    warn!("Received pass-through response with no/wrong outstanding transaction: {:?} {:?}", message, transaction);
                    return Ok(());
                }
                let _ = transaction.take_sender().send(match frame.ctype {
                    CommandCode::Accepted => Ok(message.data),
                    CommandCode::Rejected => Err(SessionError::Rejected),
                    CommandCode::NotImplemented => Err(SessionError::NotImplemented),
                    _ => Err(SessionError::InvalidReturnData)
                });
                Ok(())
            }
            code => {
                warn!("Unsupported opcode: {:?}", code);
                Err(AvcError::NotImplemented)
            }
        }
    }

    fn send_avrcp<I: Instruct<BigEndian>>(&mut self, transaction_label: u8, cmd: CommandCode, pdu: Pdu, parameters: I) -> bool {
        fragment_command(cmd, pdu, parameters, |data| {
            self.avctp.send_msg(Message {
                transaction_label,
                profile_id: REMOTE_CONTROL_SERVICE,
                message_type: match cmd.is_response() {
                    true => MessageType::Response,
                    false => MessageType::Command,
                },
                data,
            })
        }).map_err(|err| warn!("Error sending command: {:?}", err)).is_ok()
    }

    fn send_avc<I: Instruct<BigEndian>>(&mut self, transaction_label: u8, frame: Frame, parameters: I) -> bool {
        let mut buffer = BytesMut::new();
        buffer.write(frame);
        buffer.write(parameters);
        self.avctp.send_msg(Message {
            transaction_label,
            profile_id: REMOTE_CONTROL_SERVICE,
            message_type: match frame.ctype.is_response() {
                true => MessageType::Response,
                false => MessageType::Command,
            },
            data: buffer.freeze()
        }).map_err(|err| warn!("Error sending command: {:?}", err)).is_ok()
    }

    fn process_command(&mut self, transaction: u8, _cmd: CommandCode, pdu: Pdu, mut parameters: Bytes) -> Result<(), ErrorCode> {
        match pdu {
            // ([AVRCP] Section 6.4.1)
            Pdu::GetCapabilities => {
                let capability: u8 = parameters.read_be()?;
                parameters.finish()?;
                match capability {
                    COMPANY_ID_CAPABILITY => {
                        self.send_avrcp(transaction, CommandCode::Implemented, pdu, (COMPANY_ID_CAPABILITY, 1, BLUETOOTH_SIG_COMPANY_ID));
                        Ok(())
                    },
                    EVENTS_SUPPORTED_CAPABILITY => {
                        //TODO Support a second event type to conform to spec
                        self.send_avrcp(transaction, CommandCode::Implemented, pdu, (EVENTS_SUPPORTED_CAPABILITY, 1, EventId::VolumeChanged));
                        Ok(())
                    },
                    _ => {
                        warn!("Unsupported capability: {}", capability);
                        Err(ErrorCode::InvalidParameter)
                    }
                }
            },
            // ([AVRCP] Section 6.7.2)
            Pdu::RegisterNotification => {
                // ensure!(cmd == CommandCode::Notify, ErrorCode::InvalidCommand);
                let event: EventId = parameters.read_be()?;
                let _: u32 = parameters.read_be()?;
                parameters.finish()?;
                ensure!(event == EventId::VolumeChanged, ErrorCode::InvalidParameter);
                // ([AVRCP] Section 6.13.3)
                self.send_avrcp(transaction, CommandCode::Interim, pdu, (event, self.volume));
                Ok(())
            },
            // ([AVRCP] Section 6.13.2)
            Pdu::SetAbsoluteVolume => {
                self.volume = parameters.read_be()?;
                parameters.finish()?;
                info!("Volume set to: {}", self.volume.0);
                self.send_avrcp(transaction, CommandCode::Accepted, pdu, self.volume);
                Ok(())
            },
            _ => {
                warn!("Unsupported pdu: {:?}", pdu);
                Err(ErrorCode::InvalidCommand)
            }
        }
    }
}



#[derive(Default, Debug, Copy, Clone, PartialEq)]
pub struct Volume(pub f32);

impl Exstruct<BigEndian> for Volume {
    fn read_from_buffer<B: Buffer>(buffer: &mut B) -> Result<Self, instructor::Error> {
        let volume: u8 = buffer.read_be()?;
        Ok(Volume(volume as f32 / 0x7F as f32))
    }
}

impl Instruct<BigEndian> for Volume {
    fn write_to_buffer<B: BufferMut>(&self, buffer: &mut B) {
        let volume = (self.0.max(0.0).min(1.0) * 0x7F as f32).round() as u8;
        buffer.write_be(volume);
    }
}