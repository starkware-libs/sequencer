use std::future::Future;

use tokio::runtime::Handle;

use crate::executor::TaskExecutor;
#[cfg(test)]
#[path = "tokio_executor_test.rs"]
pub mod test;

#[derive(Clone)]
pub struct TokioExecutor {
    // Invariant: the handle must remain private to ensure all tasks spawned via this
    // executor originate from the same handle, maintaining control and consistency.
    handle: Handle,
}

impl TokioExecutor {
    pub fn new(handle: Handle) -> Self {
        Self { handle }
    }

    /// Spawns a task and returns a `JoinHandle`.
    ///
    /// This method is needed to allow tasks to be tracked and managed through a `JoinHandle`,
    /// enabling control over task lifecycle such as awaiting completion, cancellation, or checking
    /// results. It should be used only when the caller needs to manage the task directly, which is
    /// essential both in testing scenarios and in the actual system `main` function.
    /// Note: In most cases, where task management is not necessary, the `spawn` or
    /// `spawn_blocking` methods should be preferred.
    ///
    /// # Example
    /// ```
    /// use starknet_task_executor::tokio_executor::TokioExecutor;
    /// use tokio::runtime::Handle;
    /// use tokio::sync::oneshot;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let runtime = Handle::current();
    ///     let executor = TokioExecutor::new(runtime);
    ///
    ///     // Create a oneshot channel to simulate a task waiting for a signal.
    ///     let (_will_not_send, await_signal_that_wont_come) = oneshot::channel::<()>();
    ///
    ///     // Spawn a task that waits for the signal (which we will not send).
    ///     let handle = executor.spawn_with_handle(async move {
    ///         await_signal_that_wont_come.await.ok();
    ///     });
    ///
    ///     // Abort the task before sending the signal.
    ///     handle.abort();
    ///
    ///     assert!(handle.await.unwrap_err().is_cancelled());
    /// }
    /// ```
    pub fn spawn_with_handle<F>(&self, future: F) -> tokio::task::JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.handle.spawn(future)
    }
}

impl TaskExecutor for TokioExecutor {
    /// Note: `Tokio` catches task panics that returns them as errors, this is a `Tokio`-specific
    /// behavior.
    type SpawnBlockingError = tokio::task::JoinError;
    type SpawnError = tokio::task::JoinError;

    /// Spawns a task that may block, on a dedicated thread, preventing disruption of the async
    /// runtime.
    ///
    /// # Example
    ///
    /// ```
    /// use starknet_task_executor::executor::TaskExecutor;
    /// use starknet_task_executor::tokio_executor::TokioExecutor;
    ///
    /// tokio_test::block_on(async {
    ///     let executor = TokioExecutor::new(tokio::runtime::Handle::current());
    ///     let task = || {
    ///         // Simulate CPU-bound work (sleep/Duration from std and not tokio!).
    ///         std::thread::sleep(std::time::Duration::from_millis(100));
    ///         "FLOOF"
    ///     };
    ///     let result = executor.spawn_blocking(task).await;
    ///     assert_eq!(result.unwrap(), "FLOOF");
    /// });
    /// ```
    fn spawn_blocking<F, T>(
        &self,
        task: F,
    ) -> impl Future<Output = Result<T, Self::SpawnBlockingError>> + Send
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        self.handle.spawn_blocking(task)
    }

    /// Executes a async, non-blocking task.
    ///
    /// Note: If you need to manage the task directly through a `JoinHandle`, use
    /// [`Self::spawn_with_handle`] instead.
    ///
    /// # Example
    ///
    /// ```
    /// use starknet_task_executor::{
    ///   tokio_executor::TokioExecutor, executor::TaskExecutor
    /// };
    ///
    /// tokio_test::block_on(async {
    ///     let executor = TokioExecutor::new(tokio::runtime::Handle::current());
    ///     let future = async {
    ///         // Simulate IO-bound work (sleep/Duration from tokio!).
    ///         tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    ///         "HOPALA"
    ///     };
    ///     let result = executor.spawn(future).await;
    ///     assert_eq!(result.unwrap(), "HOPALA");
    /// });
    fn spawn<F, T>(&self, task: F) -> impl Future<Output = Result<T, Self::SpawnError>> + Send
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        self.handle.spawn(task)
    }
}
