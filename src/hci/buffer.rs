use std::fmt::{Debug, Formatter};
use smallvec::SmallVec;
use crate::hci::Error;
use crate::utils::SliceExt;

#[derive(Default)]
pub struct SendBuffer(SmallVec<[u8; 8]>);

impl SendBuffer {

    #[inline]
    pub(crate) fn set_u8(&mut self, index: usize, value: u8) {
        self.0[index] = value;
    }

    #[inline]
    pub fn u8(&mut self, value: impl Into<u8>) -> &mut Self {
        self.0.push(value.into());
        self
    }

    #[inline]
    pub fn u16(&mut self, value: impl Into<u16>) -> &mut Self {
        self.0.extend_from_slice(&value.into().to_le_bytes());
        self
    }

    #[inline]
    pub fn u24(&mut self, value: impl Into<u32>) -> &mut Self {
        let v = value.into().to_le_bytes();
        assert_eq!(v[3], 0);
        self.0.extend_from_slice(&v[..3]);
        self
    }


    #[inline]
    pub fn bytes(&mut self, bytes: &[u8]) -> &mut Self {
        self.0.extend_from_slice(bytes);
        self
    }

    pub fn pad(&mut self, n: usize) -> &mut Self {
        self.0.resize(self.0.len() + n, 0);
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

impl From<&'static str> for Error {
    fn from(value: &'static str) -> Self {
        Self::Generic(value)
    }
}

impl Debug for SendBuffer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.0.iter()).finish()
    }
}


#[derive(Default, Clone)]
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

    pub fn u8(&mut self) -> Result<u8, Error> {
        let value = self.data.get(self.index).copied().ok_or(Error::BadEventPacketSize)?;
        self.index += 1;
        Ok(value)
    }

    pub fn u16(&mut self) -> Result<u16, Error> {
        let value = self.data.get_chunk(self.index)
            .map(|bytes| u16::from_le_bytes(*bytes))
            .ok_or(Error::BadEventPacketSize)?;
        self.index += 2;
        Ok(value)
    }

    pub fn u24(&mut self) -> Result<u32, Error> {
        let value = self.data.get_chunk::<3>(self.index)
            .map(|b| (b[2] as u32) | ((b[1] as u32) << 8) | ((b[0] as u32) << 16))
            .ok_or(Error::BadEventPacketSize)?;
        self.index += 3;
        Ok(value)
    }

    pub fn array<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        let value = self.data.get_chunk(self.index)
            .copied()
            .ok_or(Error::BadEventPacketSize)?;
        self.index += N;
        Ok(value)
    }

    pub fn bytes(&mut self, n: usize) -> Result<&[u8], Error> {
        let value = self.data.get(self.index..self.index + n)
            .ok_or(Error::BadEventPacketSize)?;
        self.index += n;
        Ok(value)
    }

    pub fn finish(self) -> Result<(), Error> {
        (self.remaining() == 0)
            .then_some(())
            .ok_or(Error::BadEventPacketSize)
    }

    pub fn skip(&mut self, n: usize) {
        self.index += n;
    }

    pub(crate) fn remaining(&self) -> usize {
        self.data.len() - self.index
    }
    pub(crate) fn get_mut(&mut self) -> &mut [u8] {
        self.data[self.index..].as_mut()
    }
}

impl Debug for ReceiveBuffer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.data.iter().skip(self.index)).finish()
    }
}