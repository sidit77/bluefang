use bytes::{BufMut, Bytes, BytesMut};
use instructor::{Exstruct, Instruct};
use instructor::utils::Length;
use tracing::warn;
// use crate::l2cap::Error;
use crate::utils::SliceExt;

#[derive(Default)]
pub struct AclDataAssembler {
    buffer: BytesMut,
    l2cap_pdu_length: usize,
    in_progress: bool,
}

impl AclDataAssembler {
    pub fn push(&mut self, header: AclHeader, data: Bytes) -> Option<Bytes> {
        if header.pb.is_first() {
            debug_assert!(!self.in_progress);
            if let Some(l2cap_pdu_length) = data
                .get_chunk(0)
                .copied()
                .map(u16::from_le_bytes) {
                self.buffer.clear();
                self.buffer.put(data);
                self.l2cap_pdu_length = l2cap_pdu_length as usize;
                self.in_progress = true;
            } else {
                warn!("A start packet should contain a valid L2CAP PDU length");
                return None;
            }
        } else {
            if self.in_progress {
                self.buffer.put(data);
            } else {
                warn!("A continuation packet should not be the first packet");
                return None;
            }
        }
        debug_assert!(self.in_progress);
        match self.buffer.len().cmp(&(self.l2cap_pdu_length + 4)) {
            std::cmp::Ordering::Less => None,
            std::cmp::Ordering::Equal => {
                self.in_progress = false;
                Some(self.buffer.split().freeze())
            }
            std::cmp::Ordering::Greater => {
                warn!("L2CAP PDU length exceeded");
                self.in_progress = false;
                None
            }
        }
    }
}

// ([Vol 4] Part E, Section 5.4.2).
#[derive(Debug, Copy, Clone, Exstruct, Instruct)]
#[instructor(endian = "little")]
pub struct AclHeader {
    #[instructor(bitfield(u16))]
    #[instructor(bits(0..12))]
    pub handle: u16,
    #[instructor(bits(12..14))]
    pub pb: BoundaryFlag,
    #[instructor(bits(14..16))]
    pub bc: BroadcastFlag,
    pub length: Length<u16, 0>
}

// ([Vol 4] Part E, Section 5.4.2).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
#[repr(u8)]
pub enum BoundaryFlag {
    FirstNonAutomaticallyFlushable = 0b00,
    Continuing = 0b01,
    FirstAutomaticallyFlushable = 0b10,
}

impl BoundaryFlag {
    pub fn is_first(self) -> bool {
        matches!(self, Self::FirstNonAutomaticallyFlushable | Self::FirstAutomaticallyFlushable)
    }
}

// ([Vol 4] Part E, Section 5.4.2).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Instruct, Exstruct)]
#[repr(u8)]
pub enum BroadcastFlag {
    PointToPoint = 0b00,
    BrEdrBroadcast = 0b01,
}
