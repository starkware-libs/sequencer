use std::future::Future;

use tokio::task::JoinHandle;
use tracing::error;

#[cfg(test)]
#[path = "tasks_test.rs"]
mod tasks_test;

/// Spawns a monitored asynchronous task in Tokio.
///
/// This function spawns two tasks:
/// 1. The first task executes the provided future.
/// 2. The second task monitors the first task, awaiting its completion and logging an error if it
///    fails.
///    - If the first task succeeds then it returns its result.
///    - If the first task panics or returns an error, the process will terminate with exit code 1.
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
pub fn spawn_monitored_task<F, T>(future: F) -> JoinHandle<T>
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    inner_spawn_monitored_task(future, exit_process)
}

// Use an inner function to enable injecting the exit function for testing.
pub(crate) fn inner_spawn_monitored_task<F, E, T>(future: F, on_exit_f: E) -> JoinHandle<T>
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

pub(crate) fn exit_process() {
    std::process::exit(1);
}
