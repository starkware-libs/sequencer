use std::sync::Arc;

use tokio::sync::Semaphore;

use crate::errors::FeederGatewayError;
use crate::reader::FgResult;

#[cfg(test)]
#[path = "executor_test.rs"]
mod executor_test;

/// A bounded executor for synchronous, page-cache-bound MDBX reads (per the read-execution design).
///
/// MDBX reads must not run inline on the async reactor (a slow page-cache miss would block a worker
/// thread and starve HTTP scheduling), and they must not be dispatched to the global blocking pool
/// without a bound (its queue is unbounded, so a spike grows threads until the box thrashes). Every
/// read is therefore routed through this executor, which caps the number of concurrent blocking
/// reads with a semaphore sized ~1.5x physical cores.
///
/// Backpressure model: [`ReadExecutor::run`] AWAITS for a permit when the executor is saturated and
/// never rejects, so a full read queue simply slows request acceptance (the desired natural
/// throttle) rather than surfacing an overload error.
pub struct ReadExecutor {
    semaphore: Arc<Semaphore>,
    read_pool_size: usize,
}

impl ReadExecutor {
    pub fn new(read_pool_size: usize) -> Self {
        Self { semaphore: Arc::new(Semaphore::new(read_pool_size)), read_pool_size }
    }

    /// Runs a blocking read closure on the blocking pool, bounded by the executor's concurrency
    /// limit. Awaits a permit if the executor is saturated; never rejects. The returned
    /// `FgResult` reports only executor-level failure (the spawned task panicking); the closure's
    /// own success type carries any read error.
    pub async fn run<F, T>(&self, f: F) -> FgResult<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        // The permit is held for the duration of the blocking work, so at most `read_pool_size`
        // reads execute concurrently.
        let _permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| FeederGatewayError::Internal)?;
        tokio::task::spawn_blocking(f).await.map_err(|join_error| {
            tracing::error!(error = %join_error, "feeder gateway read task failed to join");
            FeederGatewayError::Internal
        })
    }

    /// The maximum number of concurrent reads (the semaphore size). Exposed for a saturation
    /// metric.
    pub fn max_concurrency(&self) -> usize {
        self.read_pool_size
    }

    /// The number of reads currently executing (held permits). Exposed for a saturation metric.
    pub fn in_flight(&self) -> usize {
        self.read_pool_size.saturating_sub(self.semaphore.available_permits())
    }
}
