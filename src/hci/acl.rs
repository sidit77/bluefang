use num_enum::{IntoPrimitive, TryFromPrimitive};
use tracing::warn;
use crate::ensure;
// use crate::l2cap::Error;
use crate::utils::SliceExt;

#[derive(Default)]
pub struct AclDataAssembler {
    buffer: Vec<u8>,
    l2cap_pdu_length: usize,
    in_progress: bool,
}

impl AclDataAssembler {
    pub fn push(&mut self, packet: AclDataPacket) -> Option<Vec<u8>> {
        if packet.pb.is_first() {
            debug_assert!(!self.in_progress);
            if let Some(l2cap_pdu_length) = packet.data
                .get_chunk(0)
                .copied()
                .map(u16::from_le_bytes) {
                self.buffer.clear();
                self.buffer.extend_from_slice(&packet.data);
                self.l2cap_pdu_length = l2cap_pdu_length as usize;
                self.in_progress = true;
            } else {
                warn!("A start packet should contain a valid L2CAP PDU length");
                return None;
            }
        } else {
            if self.in_progress {
                self.buffer.extend_from_slice(&packet.data);
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
                Some(std::mem::take(&mut self.buffer))
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
#[derive(Debug, Clone)]
pub struct AclDataPacket {
    pub handle: u16,
    pub pb: BoundaryFlag,
    pub bc: BroadcastFlag,
    pub data: Vec<u8>,
}

impl AclDataPacket {
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        let hdr = u16::from_le_bytes(*data.get_chunk(0)?);
        let handle = hdr & 0xFFF;
        let pb = BoundaryFlag::try_from(((hdr >> 12) & 0b11) as u8).ok()?;
        let bc = BroadcastFlag::try_from(((hdr >> 14) & 0b11) as u8).ok()?;
        let len = u16::from_le_bytes(*data.get_chunk(2)?) as usize;
        let data = &data[4..];
        ensure!(data.len() == len);
        Some(Self { handle, pb, bc, data: data.to_vec() })
    }

    pub fn into_vec(mut self) -> Vec<u8> {
        let len = self.data.len() as u16;
        let mut hdr = self.handle;
        hdr |= (self.pb as u16) << 12;
        hdr |= (self.bc as u16) << 14;
        self.data.extend_from_slice(&hdr.to_le_bytes());
        self.data.extend_from_slice(&len.to_le_bytes());
        self.data.rotate_right(4);
        self.data
    }
}

// ([Vol 4] Part E, Section 5.4.2).
#[derive(Debug, Copy, Clone, Eq, PartialEq, TryFromPrimitive, IntoPrimitive)]
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
#[derive(Debug, Copy, Clone, Eq, PartialEq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum BroadcastFlag {
    PointToPoint = 0b00,
    BrEdrBroadcast = 0b01,
}
