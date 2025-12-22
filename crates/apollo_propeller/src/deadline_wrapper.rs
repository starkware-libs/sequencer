use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use tokio::task::JoinHandle;

/// A wrapper that panics if a single poll takes too long.
pub struct DeadlineWrapper<F: Future> {
    // We store a Pinned, Boxed version of the future to avoid 'unsafe'
    inner: Pin<Box<F>>,
    max_poll_time: Duration,
    task_name: &'static str,
}

impl<F: Future> Future for DeadlineWrapper<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let start = Instant::now();

        // We can safely poll 'inner' because it is already Pinned in the Box
        let result = self.inner.as_mut().poll(cx);

        let elapsed = start.elapsed();

        if elapsed > self.max_poll_time {
            let message = format!(
                "Task violation in '{}': poll took {:?}, which exceeds limit of {:?}",
                self.task_name, elapsed, self.max_poll_time
            );
            tracing::error!("{}", message);
            // fail in debug mode
            debug_assert!(false, "{}", message);
        }

        result
    }
}

pub fn spawn_monitored<F>(task_name: &'static str, fut: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(DeadlineWrapper {
        inner: Box::pin(fut), // Pinning happens here safely
        max_poll_time: Duration::from_millis(25),
        task_name,
    })
}
