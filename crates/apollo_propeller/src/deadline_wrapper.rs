use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use tokio::task::JoinHandle;

const MAX_POLL_TIME_IN_DEBUG_MODE: Duration = Duration::from_millis(100);
const MAX_POLL_TIME: Duration = Duration::from_millis(25);

/// A wrapper that panics if a single poll takes too long.
pub struct DeadlineWrapper<F: Future> {
    // We store a Pinned, Boxed version of the future to avoid 'unsafe'
    inner: Pin<Box<F>>,
    task_name: &'static str,
}

impl<F: Future> Future for DeadlineWrapper<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let start = Instant::now();

        // We can safely poll 'inner' because it is already Pinned in the Box
        let result = self.inner.as_mut().poll(cx);

        let elapsed = start.elapsed();

        if elapsed > MAX_POLL_TIME {
            let message = format!(
                "Task violation in '{}': poll took {:?}, which exceeds limit of {:?}",
                self.task_name, elapsed, MAX_POLL_TIME
            );
            tracing::error!("{}", message);

            // fail in debug mode
            debug_assert!(elapsed <= MAX_POLL_TIME_IN_DEBUG_MODE, "{}", message);
        }

        result
    }
}

pub fn spawn_monitored<F>(task_name: &'static str, fut: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(DeadlineWrapper { inner: Box::pin(fut), task_name })
}
