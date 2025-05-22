use std::future::Future;
use std::task::{Context, Poll};

use futures::future::BoxFuture;
use futures::never::Never;
use futures::stream::FuturesUnordered;
use futures::{FutureExt, StreamExt};
use tokio::time::Instant;

/// A manager which wakes a waker at a specified time.
///
/// When using this object, it must be polled in every call to poll the returns pending.
#[derive(Default)]
pub(super) struct TimeWakerManager {
    timers: FuturesUnordered<BoxFuture<'static, ()>>,
}

impl TimeWakerManager {
    /// Will wake the context at the instant.
    pub fn wake_at(&mut self, cx: &mut Context<'_>, instant: Instant) {
        let waker = cx.waker().clone();
        let future = async move {
            tokio::time::sleep_until(instant).await;
            waker.wake();
        }
        .boxed();

        // add the future to the list of timers
        self.timers.push(future);
        let _ = self.timers.poll_next_unpin(cx);
    }
}

impl Future for TimeWakerManager {
    type Output = Never;

    /// This method will always return pending. Despite this it should continue to be called every
    /// time the caller of this class' is polled.
    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> std::task::Poll<Self::Output> {
        let this = self.get_mut();
        let _ = this.timers.poll_next_unpin(cx);
        Poll::Pending
    }
}
