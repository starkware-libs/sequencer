//! A utility for managing Tokio tasks with added safety.
//!
//! The `ProtectedJoinHandle` ensures that any task it manages is explicitly resolved
//! before being dropped. This helps avoid silent task failures by enforcing
//! task completion, cancellation, or handling.
//!
//! Key Features:
//! - Ensures tasks are resolved or explicitly aborted before the handle is dropped.
//! - Provides `Future` implementation for awaiting task results.
//! - Logs and handles panics in tasks gracefully.
//!
//! Example usage:
//! ```ignore
//! let handle = spawn_protected(async { some_async_work().await });
//! handle.abort(); // Explicitly aborts the task
//! ```

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::task::{JoinError, JoinHandle};
use tracing::error;

#[cfg(test)]
#[path = "tasks_test.rs"]
mod tasks_test;

/// A constant message used when a `ProtectedJoinHandle` is dropped without being resolved.
pub(crate) const UNRESOLVED_DROP_MESSAGE: &str = "Unresolved ProtectedJoinHandle dropped";

/// Spawns a monitored asynchronous task in Tokio.
///
/// This function spawns two tasks:
/// 1. The first task executes the provided future.
/// 2. The second task awaits the completion of the first task.
///    - If the first task completes successfully, it returns its result.
///    - If the first task panics, it logs the error and terminates the process with exit code 1.
///
/// # Type Parameters
///
/// - `F`: The type of the future to be executed. Must implement `Future` and be `Send + 'static`.
/// - `T`: The output type of the future. Must be `Send + 'static`.
///
/// # Arguments
///
/// - `future`: The future to be executed by the spawned task.
///
/// # Returns
///
/// A `JoinHandle<T>` of the second monitoring task.
pub fn spawn_with_exit_on_panic<F, T>(future: F) -> JoinHandle<T>
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    inner_spawn_with_exit_on_panic(future, exit_process)
}

/// Inner function for spawning monitored tasks, allowing injection of the exit function for
/// testing.
///
/// # Type Parameters
///
/// - `F`: The type of the future to be executed. Must implement `Future` and be `Send + 'static`.
/// - `E`: The type of the exit function. Must be a callable function or closure.
/// - `T`: The output type of the future. Must be `Send + 'static`.
///
/// # Arguments
///
/// - `future`: The future to be executed by the first task.
/// - `on_exit_f`: A function to be called when the first task panics.
///
/// # Returns
///
/// A `JoinHandle<T>` of the second monitoring task.
pub(crate) fn inner_spawn_with_exit_on_panic<F, E, T>(future: F, on_exit_f: E) -> JoinHandle<T>
where
    F: Future<Output = T> + Send + 'static,
    E: FnOnce() + Send + 'static,
    T: Send + 'static,
{
    // Spawn the first task to execute the future
    let monitored_task = tokio::spawn(future);

    // Spawn the second task to await the first task and assert its completion
    tokio::spawn(async move {
        match monitored_task.await {
            Ok(res) => res,
            Err(err) => {
                error!("Monitored task failed: {:?}", err);
                on_exit_f();
                unreachable!()
            }
        }
    })
}

/// Terminates the process with exit code 1.
pub(crate) fn exit_process() {
    std::process::exit(1);
}

/// A `JoinHandle` wrapper that ensures:
/// 1. tasks are explicitly resolved; dropping an unresolved instance will cause a panic.
/// 2. panics are logged, and propagated to the invoking task.
pub struct ProtectedJoinHandle<T> {
    /// The inner task `JoinHandle`.
    handle: JoinHandle<T>,

    /// Tracks whether the task has been resolved (completed or aborted).
    resolved: bool,
}

impl<T> ProtectedJoinHandle<T> {
    /// Creates a new `ProtectedJoinHandle`.
    ///
    /// # Arguments
    /// * `handle` - The `JoinHandle` representing the spawned task.
    fn new(handle: JoinHandle<T>) -> Self {
        Self { handle, resolved: false }
    }

    /// Aborts the associated task and marks it as resolved.
    ///
    /// This method cancels the task and ensures that the handle is marked
    /// as resolved to avoid triggering a panic in the `Drop` implementation. Note that similarly to
    /// [`JoinHandle::abort`], this method does not guarantee that the task will be immediately
    /// stopped. Additionally, aborted tasks are not checked for completion, and specifically not
    /// for panic termination. As such, panics in aborted tasks is not propagated to the invoker.
    pub fn abort(&mut self) {
        self.handle.abort();
        self.resolved = true;
    }
}

impl<T> Drop for ProtectedJoinHandle<T> {
    /// Ensures that the task is resolved before dropping the handle.
    ///
    /// Panics if the task is dropped without being resolved (awaited or aborted).
    fn drop(&mut self) {
        assert!(self.resolved, "{UNRESOLVED_DROP_MESSAGE}");
    }
}

impl<T> Future for ProtectedJoinHandle<T> {
    type Output = std::result::Result<T, JoinError>;

    /// Polls the inner task and resolves its result.
    ///
    /// This implementation ensures that the `resolved` flag is updated
    /// once the task completes, even if it panics or fails.
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut(); // Access the inner value safely
        let poll_result = Pin::new(&mut this.handle).poll(cx);

        match poll_result {
            Poll::Ready(result) => {
                this.resolved = true;

                // Examine the result, log and propagate panics .
                if let Err(err) = result.as_ref() {
                    if err.is_panic() {
                        error!("ProtectedJoinHandle task panicked: {:?}", err);
                        panic!("Task panicked: {:?}", err);
                    }
                }
                Poll::Ready(result)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Spawns a protected asynchronous task.
///
/// This function wraps a `tokio::spawn` call, returning a `ProtectedJoinHandle`.
///
/// # Type Parameters
///
/// - `F`: The type of the future to be executed. Must implement `Future` and be `Send + 'static`.
/// - `T`: The output type of the future. Must be `Send + 'static`.
///
/// # Arguments
///
/// - `future`: The asynchronous task to be spawned.
///
/// # Returns
///
/// A `ProtectedJoinHandle<T>` for the spawned task.
pub fn spawn_protected<F, T>(future: F) -> ProtectedJoinHandle<T>
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let handle = tokio::spawn(future);
    ProtectedJoinHandle::new(handle)
}
