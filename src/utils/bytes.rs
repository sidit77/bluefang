use bytes::{Bytes, BytesMut};
use instructor::{BigEndian, BufferMut, Instruct};

pub trait SliceExt<T> {
    fn get_chunk<const N: usize>(&self, index: usize) -> Option<&[T; N]>;
}

impl<T> SliceExt<T> for [T] {
    fn get_chunk<const N: usize>(&self, index: usize) -> Option<&[T; N]> {
        //self
        //    .get(index..index + N)
        //    .map(|slice| unsafe { &*(slice.as_ptr().cast::<[T; N]>()) })
        self.get(index..)
            .and_then(|slice| slice.split_first_chunk().map(|(a, _)| a))
    }
}

pub fn to_bytes_be<I: Instruct<BigEndian>>(value: I) -> Bytes {
    let mut buffer = BytesMut::new();
    buffer.write_be(&value);
    buffer.freeze()
}