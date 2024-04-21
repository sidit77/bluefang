
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