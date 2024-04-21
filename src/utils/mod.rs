mod iter;
mod bytes;

pub use iter::IteratorExt;
pub use bytes::SliceExt;

#[macro_export]
macro_rules! ensure {
    ($cond:expr) => {
        if !($cond) {
            return None;
        }
    };
    ($cond:expr, $err:expr) => {
        if !($cond) {
            return Err($err);
        }
    };
}