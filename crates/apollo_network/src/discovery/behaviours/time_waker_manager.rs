use std::task::Context;

use futures::future::BoxFuture;
use futures::stream::FuturesUnordered;
use futures::{FutureExt, StreamExt};
use tokio::time::Instant;

/// A manager for scheduling contexts to wake at specific times.
#[derive(Default)]
pub(super) struct TimeWakerManager {
    timers: FuturesUnordered<BoxFuture<'static, ()>>,
}

impl TimeWakerManager {
    /// Will wake the context at a specific instant.
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
        let _ = self.timers.poll_next_unpin(cx);
    }
}
