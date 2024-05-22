use std::future::Future;

/// An abstraction for executing tasks, suitable for both CPU-bound and I/O-bound operations.
pub trait TaskExecutor {
    type SpawnBlockingError;
    type SpawnError;

    /// Offloads a blocking task, _ensuring_ the async event loop remains responsive.
    /// It accepts a function that executes a blocking operation and returns a result.
    fn spawn_blocking<F, T>(
        &self,
        task: F,
    ) -> impl Future<Output = Result<T, Self::SpawnBlockingError>> + Send
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static;

    /// Offloads a non-blocking task asynchronously.
    /// It accepts a future representing an asynchronous operation and returns a result.
    fn spawn<F, T>(&self, task: F) -> impl Future<Output = Result<T, Self::SpawnError>> + Send
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static;
}
