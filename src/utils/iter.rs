
pub trait IteratorExt<T> {
    fn single(self) -> Option<T> where Self: Sized;
}

impl<T, I: Iterator<Item=T>> IteratorExt<T> for I {
    fn single(mut self) -> Option<T> where Self: Sized {
        match (self.next(), self.next()) {
            (Some(e), None) => Some(e),
            _ => None
        }
    }
}