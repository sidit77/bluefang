use std::fmt::{Debug, Formatter};
use smallvec::SmallVec;
use crate::utils::SliceExt;

#[derive(Default)]
pub struct SendBuffer(SmallVec<[u8; 8]>);

impl SendBuffer {

    #[inline]
    pub(crate) fn set_u8(&mut self, index: usize, value: u8) {
        self.0[index] = value;
    }

    #[inline]
    pub fn put_u8(&mut self, value: impl Into<u8>) -> &mut Self {
        self.0.push(value.into());
        self
    }

    #[inline]
    pub fn put_u16(&mut self, value: impl Into<u16>) -> &mut Self {
        self.0.extend_from_slice(&value.into().to_le_bytes());
        self
    }

    /// Dummy method to end a chain with unit type
    /// Can be helpful with closures: `|b| b.put_u8(12).end()`
    pub fn end(&mut self) { }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn data(&self) -> &[u8] {
        &self.0
    }

}

impl Debug for SendBuffer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.0.iter()).finish()
    }
}


#[derive(Default)]
pub struct ReceiveBuffer {
    data: SmallVec<[u8; 8]>,
    index: usize
}

impl ReceiveBuffer {

    pub(crate) fn from_payload(data: &[u8]) -> Self {
        Self {
            data: SmallVec::from_slice(data),
            index: 0
        }
    }

    pub fn get_u8(&mut self) -> Option<u8> {
        let value = self.data.get(self.index).copied();
        self.index += 1;
        value
    }

    pub fn get_u16(&mut self) -> Option<u16> {
        let value = self.data.get_chunk(self.index)
            .map(|bytes| u16::from_le_bytes(*bytes));
        self.index += 2;
        value
    }

    fn remaining(&self) -> usize {
        self.data.len() - self.index
    }
}

impl Debug for ReceiveBuffer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.data.iter().skip(self.index)).finish()
    }
}