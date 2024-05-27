mod iter;
mod bytes;
mod mutex_cell;

use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc::UnboundedSender;
pub use iter::IteratorExt;
pub use bytes::SliceExt;
pub use mutex_cell::MutexCell;

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
    n: usize,
}

impl<T: Clone> Iterator for RepeatN<T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        self.n -= 1;
        if self.n > 0 {
            self.value.clone()
        } else {
            self.value.take()
        }
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


pub struct SelectAll<'a, T> {
    futures: &'a mut [T],
}

impl<'a, T: Future + Unpin> Future for SelectAll<'a, T> {
    type Output = (usize, T::Output);

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        for (i, future) in self.futures.iter_mut().enumerate() {
            if let Poll::Ready(output) = Pin::new(future).poll(cx) {
                return Poll::Ready((i, output));
            }
        }
        Poll::Pending
    }
}

pub fn select_all<T: Future + Unpin>(futures: &mut [T]) -> SelectAll<T> {
    SelectAll { futures }
}

pub async fn stall_if_none<F: Future + Unpin>(fut: &mut Option<F>) -> F::Output {
    match fut {
        Some(fut) => fut.await,
        None => std::future::pending().await
    }
}