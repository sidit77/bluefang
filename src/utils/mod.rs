mod iter;
mod bytes;

use std::fmt::{Debug, Formatter};
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
            return Err($err.into());
        }
    };
}

#[macro_export]
macro_rules! log_assert {
    ($cond:expr) => {
        if !($cond) {
            tracing::warn!("Assertion failed: {}", stringify!($cond));
        }
    };
}

pub struct DebugFn<F: Fn(&mut Formatter<'_>) -> std::fmt::Result>(pub F);

impl<F: Fn(&mut Formatter<'_>) -> std::fmt::Result> Debug for DebugFn<F> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        (self.0)(f)
    }
}