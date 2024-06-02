use std::collections::BTreeSet;
use std::io::BufRead;
use std::sync::Arc;
use bytes::{Bytes, BytesMut};
use instructor::{BigEndian, Buffer, BufferMut, Exstruct, Instruct};
use instructor::utils::u24;
use parking_lot::Mutex;
use tokio::{select, spawn};
use tokio::sync::mpsc::Receiver;
use tracing::{info, trace, warn};
use crate::avc::{CommandCode, Frame, Opcode, PassThroughFrame, PassThroughOp, PassThroughState, Subunit, SubunitType};
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

enum PlayerCommand {
    Play,
    Pause
}

fn command_reader() -> Receiver<PlayerCommand> {
    let (tx, rx) = tokio::sync::mpsc::channel(10);
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(std::io::stdin()).lines() {
            match line.expect("Failed to read line").to_lowercase().as_str() {
                "play" => tx.blocking_send(PlayerCommand::Play).unwrap(),
                "pause" => tx.blocking_send(PlayerCommand::Pause).unwrap(),
                _ => continue,
            }
        }
    });
    rx
}

#[derive(Default)]
pub struct AvrcpBuilder;

impl AvrcpBuilder {
    pub fn build(self) -> Avrcp {
        Avrcp {
            existing_connections: Arc::new(Mutex::new(BTreeSet::new()))
        }
    }
}

#[derive(Clone)]
pub struct Avrcp {
    existing_connections: Arc<Mutex<BTreeSet<u16>>>
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
            spawn(async move {
                if let Err(err) = channel.configure().await {
                    warn!("Error configuring channel: {:?}", err);
                    return;
                }
                let mut state = State {
                    avctp: Avctp::new(channel, [REMOTE_CONTROL_SERVICE]),
                    command_assembler: Default::default(),
                    response_assembler: Default::default(),
                    volume: Default::default(),
                    next_transaction: 0,
                    player_commands: command_reader(),
                };
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

    next_transaction: u8,

    player_commands: Receiver<PlayerCommand>
}

impl State {
    async fn run(&mut self) -> Result<(), hci::Error> {
        loop {
            select! {
                Some(mut packet) = self.avctp.read() => {
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
                Some(cmd) = self.player_commands.recv() => {
                    match cmd {
                        PlayerCommand::Play => {
                            self.send_pass_through(PassThroughOp::Play, PassThroughState::Pressed);
                            self.send_pass_through(PassThroughOp::Play, PassThroughState::Released);
                        },
                        PlayerCommand::Pause => {
                            self.send_pass_through(PassThroughOp::Pause, PassThroughState::Pressed);
                            self.send_pass_through(PassThroughOp::Pause, PassThroughState::Released);
                        }
                    }
                },
                else => break
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
                let ptf: PassThroughFrame = message.data.read_be()?;
                info!("Received pass-through frame: {:?}", ptf);

                Ok(())
            }
            code => {
                warn!("Unsupported opcode: {:?}", code);
                Err(AvcError::NotImplemented)
            }
        }
    }

    fn send_avrcp<I: Instruct<BigEndian>>(&mut self, transaction_label: u8, cmd: CommandCode, pdu: Pdu, parameters: I) {
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
        }).unwrap_or_else(|err| {
            warn!("Error sending command: {:?}", err);
        });
    }

    fn send_avc<I: Instruct<BigEndian>>(&mut self, transaction_label: u8, frame: Frame, parameters: I) {
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
        }).unwrap_or_else(|err| {
            warn!("Error sending command: {:?}", err);
        });
    }

    fn get_transaction_label(&mut self) -> u8 {
        let next = self.next_transaction;
        self.next_transaction = (self.next_transaction + 1) % 16;
        next
    }

    fn send_pass_through(&mut self, op: PassThroughOp, state: PassThroughState) {
        let tl = self.get_transaction_label();
        self.send_avc(tl,
            Frame {
                ctype: CommandCode::Control,
                subunit: PANEL,
                opcode: Opcode::PassThrough,
            },
            PassThroughFrame { op, state, data_len: 0 }
        )
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