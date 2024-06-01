use std::collections::BTreeSet;
use std::sync::Arc;
use bytes::Bytes;
use instructor::{BigEndian, Buffer, BufferMut, Exstruct, Instruct};
use instructor::utils::u24;
use parking_lot::Mutex;
use tokio::spawn;
use tracing::{info, trace, warn};
use crate::avc::{CommandCode, Frame, Opcode};
use crate::avctp::{Avctp, Message, MessageType};
use crate::avrcp::sdp::REMOTE_CONTROL_SERVICE;
use crate::{ensure, hci};
use crate::avrcp::error::{Error2, ErrorCode};
use crate::avrcp::packets::{BLUETOOTH_SIG_COMPANY_ID, Command, CommandAssembler, Event, fragment_command, PANEL, Pdu};
use crate::l2cap::channel::Channel;
use crate::l2cap::{AVCTP_PSM, ProtocolDelegate, ProtocolHandler, ProtocolHandlerProvider};

pub mod sdp;
mod packets;
mod error;

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
}

impl State {
    async fn run(&mut self) -> Result<(), hci::Error> {
        while let Some(packet) = self.avctp.read().await {
            self.process_message(packet)
                .unwrap_or_else(|err| warn!("Error processing message: {:?}", err));
        }
        Ok(())
    }

    fn process_message(&mut self, mut message: Message) -> Result<(), Error2> {
        let avc_frame: Frame = message.data.read()?;
        ensure!(avc_frame.subunit == PANEL, Error2::NotImplemented, "Unsupported subunit: {:?}", avc_frame.subunit);
        match avc_frame.opcode {
            Opcode::VendorDependent => {
                let company_id: u24 = message.data.read_be::<u24>()?;
                ensure!(company_id == BLUETOOTH_SIG_COMPANY_ID, Error2::NotImplemented, "Unsupported company id: {:#06x}", company_id);
                if avc_frame.ctype.is_response() {
                    if let Some(Command { pdu, parameters }) = self.response_assembler.process_msg(message.data)? {
                        //self.process_command(message.transaction_label, avc_frame.ctype, pdu, parameters)?;
                        info!("Received response: {:?} ({} bytes)", pdu, parameters.len());
                    }
                } else {
                    if let Some(Command { pdu, parameters }) = self.command_assembler.process_msg(message.data)? {
                        self.process_command(message.transaction_label, avc_frame.ctype, pdu, parameters)?;
                    }
                }

                Ok(())
            }
            // TODO Support pass-through frames
            //Opcode::PassThrough => {
            //    Ok(())
            //}
            code => {
                warn!("Unsupported opcode: {:?}", code);
                Err(Error2::NotImplemented)
            }
        }
    }

    fn send_command<I: Instruct<BigEndian>>(&mut self, transaction_label: u8, cmd: CommandCode, pdu: Pdu, parameters: I) {
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
                self.send_command(transaction, CommandCode::Interim, pdu, (event, self.volume));
                Ok(())
            },
            // ([AVRCP] Section 6.13.2)
            Pdu::SetAbsoluteVolume => {
                self.volume = parameters.read_be()?;
                parameters.finish()?;
                info!("Volume set to: {}", self.volume.0);
                self.send_command(transaction, CommandCode::Accepted, pdu, self.volume);
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