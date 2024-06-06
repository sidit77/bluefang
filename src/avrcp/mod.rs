use std::collections::BTreeSet;
use std::sync::Arc;
use bytes::{Bytes, BytesMut};
use instructor::{BigEndian, Buffer, BufferMut, Exstruct, Instruct};
use instructor::utils::u24;
use parking_lot::Mutex;
use tokio::{spawn};
use tokio::sync::mpsc::Receiver;
use tokio::sync::oneshot::{Sender as OneshotSender};
use tracing::{info, trace, warn};
use crate::avc::{CommandCode, Frame, Opcode, PassThroughFrame, Subunit, SubunitType};
use crate::avctp::{Avctp, Message, MessageType};
use crate::avrcp::sdp::REMOTE_CONTROL_SERVICE;
use crate::{ensure, hci};
use crate::avrcp::error::{AvcError, ErrorCode};
use crate::avrcp::packets::{BLUETOOTH_SIG_COMPANY_ID, Command, CommandAssembler, Event, fragment_command, PANEL, Pdu};
use crate::l2cap::channel::Channel;
use crate::l2cap::{AVCTP_PSM, ProtocolDelegate, ProtocolHandler, ProtocolHandlerProvider};

pub mod sdp;
mod packets;
mod error;
mod session;

pub use session::{AvrcpSession, AvrcpCommand};
use crate::avrcp::session::SessionError;
use crate::utils::{Either2, select2};


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
                let (tx, rx) = tokio::sync::mpsc::channel(16);
                let mut state = State {
                    avctp: Avctp::new(channel, [REMOTE_CONTROL_SERVICE]),
                    command_assembler: Default::default(),
                    response_assembler: Default::default(),
                    volume: Default::default(),
                    player_commands: rx,
                    outstanding_transactions: Default::default(),
                };
                session_handler.lock()(AvrcpSession { player_commands: tx });
                state.run().await.unwrap_or_else(|err| {
                    warn!("Error running avctp: {:?}", err);
                });
                trace!("AVCTP connection closed");
                existing_connections.lock().remove(&handle);
            });
        }
    }
}



struct State {
    avctp: Avctp,
    command_assembler: CommandAssembler,
    response_assembler: CommandAssembler,

    volume: Volume,

    player_commands: Receiver<(AvrcpCommand, OneshotSender<Result<Bytes, SessionError>>)>,
    outstanding_transactions: [Option<(Opcode, OneshotSender<Result<Bytes, SessionError>>)>; 16]
}

impl State {
    async fn run(&mut self) -> Result<(), hci::Error> {
        loop {
            match select2(self.avctp.read(), self.player_commands.recv()).await {
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
                        .position(|x| x.is_none())
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
                                .then(|| self.outstanding_transactions[transaction] = Some((Opcode::PassThrough, sender)));
                        },
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
                    if let Some(Command { pdu, parameters }) = self.response_assembler.process_msg(message.data)? {
                        //self.process_command(message.transaction_label, avc_frame.ctype, pdu, parameters)?;
                        info!("Received response: {:?} ({} bytes)", pdu, parameters.len());
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
                ensure!(matches!(frame.ctype, CommandCode::Accepted | CommandCode::Rejected | CommandCode::NotImplemented),
                    AvcError::NotImplemented, "Unsupported command type: {:?}", frame.ctype);
                let transaction = &mut self.outstanding_transactions[message.transaction_label as usize];
                if !matches!(transaction, Some((Opcode::PassThrough, _))) {
                    warn!("Received pass-through response with no/wrong outstanding transaction: {:?} {:?}", message, transaction);
                    return Ok(());
                }
                let (_, sender) = transaction.take().unwrap();
                let _ = sender.send(match frame.ctype {
                    CommandCode::Accepted => Ok(message.data),
                    CommandCode::Rejected => Err(SessionError::Rejected),
                    CommandCode::NotImplemented => Err(SessionError::NotImplemented),
                    _ => unreachable!()
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
        buffer.write(&frame);
        buffer.write(&parameters);
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
            // ([AVRCP] Section 6.7.2)
            Pdu::RegisterNotification => {
                // ensure!(cmd == CommandCode::Notify, ErrorCode::InvalidCommand);
                let event: Event = parameters.read_be()?;
                let _: u32 = parameters.read_be()?;
                parameters.finish()?;
                ensure!(event == Event::VolumeChanged, ErrorCode::InvalidParameter);
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
        buffer.write_be(&volume);
    }
}