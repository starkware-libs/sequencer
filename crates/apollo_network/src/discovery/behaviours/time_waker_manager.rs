use std::future::Future;
use std::task::{Context, Poll};

use futures::future::BoxFuture;
use futures::never::Never;
use futures::stream::FuturesUnordered;
use futures::{FutureExt, StreamExt};
use tokio::time::Instant;

/// A manager which wakes a waker at a specified time.
#[derive(Default)]
pub(super) struct TimeWakerManager {
    timers: FuturesUnordered<BoxFuture<'static, ()>>,
}

impl TimeWakerManager {
    /// Will wake the context at a specific instant.
    /// This function is must be followed by polling this manager in every call to poll.
    pub fn wake_at(&mut self, cx: &mut Context<'_>, instant: Instant) {
        let waker = cx.waker().clone();
        let mut future = async move {
            tokio::time::sleep_until(instant).await;
            waker.wake();
        }
        .boxed();

        // poll the future to register its waker
        let _ = future.poll_unpin(cx);

        // add the future to the list of timers
        self.timers.push(future);
    }
}

impl Future for TimeWakerManager {
    type Output = Never;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> std::task::Poll<Self::Output> {
        let this = self.get_mut();
        let _ = this.timers.poll_next_unpin(cx);
        Poll::Pending
    }
}
