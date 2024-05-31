mod packets;

use std::collections::BTreeSet;
use bytes::Bytes;
use tracing::{debug, warn};
use crate::avctp::packets::{ControlChannelExt, MessageAssembler};
use crate::l2cap::channel::Channel;
use crate::sdp::Uuid;

pub use packets::{MessageType, Message};

pub struct Avctp {
    channel: Channel,
    assembler: MessageAssembler,
    profile_ids: BTreeSet<Uuid>
}

impl Avctp {

    pub fn new<I: IntoIterator<Item=Uuid>>(channel: Channel, profiles: I) -> Self {
        Self {
            channel,
            assembler: MessageAssembler::default(),
            profile_ids: profiles.into_iter().collect()
        }
    }

    pub async fn read(&mut self) -> Option<Message> {
        while let Some(packet) = self.channel.read().await {
            match self.assembler.process_msg(packet) {
                Ok(Some(msg)) => {
                    if self.profile_ids.contains(&msg.profile_id) {
                        return Some(msg)
                    }
                    debug!("Received message with unexpected profile id: {:?}", msg.profile_id);
                    if msg.message_type == MessageType::Command {
                        self.channel.send_msg(Message {
                            transaction_label: msg.transaction_label,
                            message_type: MessageType::ResponseInvalidProfile,
                            profile_id: msg.profile_id,
                            data: Bytes::new()
                        }).unwrap_or_else(|err| {
                            warn!("Failed to send invalid profile message: {:?}", err);
                        })
                    }
                },
                Ok(None) => continue,
                Err(err) => {
                    warn!("Error processing message: {:?}", err);
                    continue;
                }
            }
        }
        None
    }

}

