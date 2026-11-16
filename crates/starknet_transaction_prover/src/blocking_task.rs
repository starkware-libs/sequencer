//! Runs blocking work on the tokio blocking pool without losing the caller's tracing span.

use tokio::task::JoinHandle;
use tracing::Span;

#[cfg(test)]
#[path = "blocking_task_test.rs"]
mod blocking_task_test;

/// Runs `task` on the tokio blocking pool inside the caller's current tracing span.
///
/// Blocking pool threads start with no span entered. Without this, logs emitted by `task` would
/// lose the fields of the enclosing spans, such as the request id and transaction hash.
#[track_caller]
pub(crate) fn spawn_blocking_in_current_span<F, T>(task: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let span = Span::current();
    tokio::task::spawn_blocking(move || span.in_scope(task))
}
