use smallvec::SmallVec;

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
