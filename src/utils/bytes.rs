use bytes::{Bytes, BytesMut};
use instructor::{BigEndian, BufferMut, Endian, Instruct, LittleEndian};

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

pub trait FromStruct {
    fn from_struct<E: Endian, I: Instruct<E>>(value: I) -> Self;
    fn from_struct_be<I: Instruct<BigEndian>>(value: I) -> Self
    where
        Self: Sized
    {
        Self::from_struct::<BigEndian, I>(value)
    }
    fn from_struct_le<I: Instruct<LittleEndian>>(value: I) -> Self
    where
        Self: Sized
    {
        Self::from_struct::<LittleEndian, I>(value)
    }
}

impl FromStruct for Bytes {
    fn from_struct<E: Endian, I: Instruct<E>>(value: I) -> Self {
        let mut buffer = BytesMut::new();
        buffer.write::<I, E>(value);
        buffer.freeze()
    }
}
