use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use pin_project_lite::pin_project;

use crate::log_assert;

#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct SelectAll<F> {
    inner: Vec<F>,
}

impl<F: Unpin> Unpin for SelectAll<F> {}

impl<F: Future + Unpin> Future for SelectAll<F> {
    type Output = (usize, F::Output);

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        for (i, future) in self.inner.iter_mut().enumerate() {
            if let Poll::Ready(output) = Pin::new(future).poll(cx) {
                return Poll::Ready((i, output));
            }
        }
        Poll::Pending
    }
}

impl<F: Future + Unpin> FromIterator<F> for SelectAll<F> {
    fn from_iter<T: IntoIterator<Item=F>>(iter: T) -> Self {
        select_all(iter)
    }
}

pub fn select_all<I>(iter: I) -> SelectAll<I::Item>
    where
        I: IntoIterator,
        I::Item: Future + Unpin,
{
    SelectAll { inner: iter.into_iter().collect() }
}

//pub struct SelectAll<'a, T> {
//    futures: &'a mut [T]
//}
//
//impl<'a, T: Future + Unpin> Future for SelectAll<'a, T> {
//    type Output = (usize, T::Output);
//
//    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
//        for (i, future) in self.futures.iter_mut().enumerate() {
//            if let Poll::Ready(output) = Pin::new(future).poll(cx) {
//                return Poll::Ready((i, output));
//            }
//        }
//        Poll::Pending
//    }
//}
//
//pub fn select_all<T: Future + Unpin>(futures: &mut [T]) -> SelectAll<T> {
//    SelectAll { futures }
//}

pin_project! {
    #[derive(Default, Debug, Copy, Clone, Eq, PartialEq)]
    #[project = OptionFutureProj]
    pub enum OptionFuture<F>{
        #[default]
        Never,
        On {
            #[pin]
            future: F
        }
    }
}

impl<F> OptionFuture<F> {
    pub const fn never() -> Self {
        OptionFuture::Never
    }

    pub fn clear(&mut self) {
        *self = OptionFuture::Never;
    }
}

impl<F: Future> OptionFuture<F> {
    pub fn set(&mut self, future: F) {
        log_assert!(matches!(self, OptionFuture::Never));
        *self = OptionFuture::On { future }
    }
}

impl<F: Future> Future for OptionFuture<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.as_mut().project() {
            OptionFutureProj::Never => Poll::Pending,
            OptionFutureProj::On { future } => match future.poll(cx) {
                Poll::Ready(r) => {
                    self.set(OptionFuture::Never);
                    Poll::Ready(r)
                }
                Poll::Pending => Poll::Pending
            }
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Either2<A, B> {
    A(A),
    B(B)
}

impl<A, B> Either2<Option<A>, Option<B>> {
    pub fn transpose(self) -> Option<Either2<A, B>> {
        match self {
            Either2::A(Some(a)) => Some(Either2::A(a)),
            Either2::B(Some(b)) => Some(Either2::B(b)),
            _ => None
        }
    }
}

pin_project! {
    #[derive(Debug)]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct Select2<F1, F2> {
        #[pin]
        future1: F1,
        #[pin]
        future2: F2,
    }
}

impl<F1, F2> Future for Select2<F1, F2>
where
    F1: Future,
    F2: Future
{
    type Output = Either2<F1::Output, F2::Output>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        if let Poll::Ready(t) = this.future1.poll(cx) {
            return Poll::Ready(Either2::A(t));
        }
        if let Poll::Ready(t) = this.future2.poll(cx) {
            return Poll::Ready(Either2::B(t));
        }
        Poll::Pending
    }
}

pub fn select2<F1, F2>(future1: F1, future2: F2) -> Select2<F1, F2>
where
    F1: Future,
    F2: Future
{
    Select2 { future1, future2 }
}
