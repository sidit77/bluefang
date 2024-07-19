mod bytes;
mod futures;
mod iter;
mod mutex_cell;

use std::fmt::{Debug, Display, Formatter};

pub use bytes::{FromStruct, SliceExt};
pub use futures::*;
pub use iter::IteratorExt;
pub use mutex_cell::MutexCell;
use tokio::sync::mpsc::UnboundedSender;
use tracing::warn;

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
    ($cond:expr, $err:expr, $($arg:tt)+) => {
        if !($cond) {
            tracing::warn!($($arg)+);
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

pub trait DispatchExt<T> {
    fn dispatch(&mut self, value: T) -> bool;
}

impl<T: Clone> DispatchExt<T> for Vec<UnboundedSender<T>> {
    fn dispatch(&mut self, value: T) -> bool {
        let mut values = repeat_n(value, self.len());
        self.retain_mut(|tx| tx.send(values.next().unwrap()).is_ok());
        !self.is_empty()
    }
}

pub fn repeat_n<T: Clone>(value: T, n: usize) -> RepeatN<T> {
    RepeatN { value: Some(value), n }
}
pub struct RepeatN<T> {
    value: Option<T>,
    n: usize
}

impl<T: Clone> Iterator for RepeatN<T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        self.n -= 1;
        if self.n > 0 { self.value.clone() } else { self.value.take() }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.n, Some(self.n))
    }
}

impl<T: Clone> DoubleEndedIterator for RepeatN<T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.next()
    }
}

impl<T: Clone> ExactSizeIterator for RepeatN<T> {}

pub fn catch_error<F, E, R>(f: F) -> Result<R, E>
where
    F: FnOnce() -> Result<R, E>
{
    f()
}

pub trait Loggable: Display {
    fn should_log(&self) -> bool;
}

impl<T> Loggable for tokio::sync::mpsc::error::TrySendError<T> {
    fn should_log(&self) -> bool {
        matches!(self, tokio::sync::mpsc::error::TrySendError::Full(_))
    }
}

impl<T> Loggable for tokio::sync::mpsc::error::SendError<T> {
    fn should_log(&self) -> bool {
        false
    }
}


pub trait LoggableResult<T, E> {
    fn log_err(self) -> Result<T, E>;
}

impl<T, E: Loggable> LoggableResult<T, E> for Result<T, E> {

    #[track_caller]
    fn log_err(self) -> Result<T, E> {
        if let Err(e) = &self {
            if e.should_log() {
                warn!("Unexpected error at {}: {}", std::panic::Location::caller(), e);
            }
        }
        self
    }

}

pub trait IgnoreableResult<E> {
    fn ignore(self);
}

impl<E: Loggable> IgnoreableResult<E> for Result<(), E> {
    #[track_caller]
    fn ignore(self) {
        let _ = self.log_err();
    }

}
/*

pub struct InstructFn<F>(F);

impl<E, B, F> Instruct<E> for InstructFn<F>
    where
        E: Endian,
        B: BufferMut,
        F: Fn(&mut B)
{
    fn write_to_buffer<B: BufferMut>(&self, buffer: &mut B) {
        self.0(buffer);
    }
}

 */
