use rstest::{fixture, rstest};
use tokio::runtime::Handle;

use crate::executor::TaskExecutor;
use crate::tokio_executor::TokioExecutor;

#[fixture]
fn executor() -> TokioExecutor {
    TokioExecutor::new(Handle::current())
}

#[rstest]
#[tokio::test]
async fn test_panic_catching(executor: TokioExecutor) {
    // Assert that panic in a task is caught and wrapped in an error.
    assert!(executor.spawn_blocking(|| panic!()).await.is_err());
    // Ensure the executor remained usable after the worker thread panicked.
    assert!(executor.spawn_blocking(|| "Real tasks don't panic").await.is_ok());

    // Ditto for async tasks.
    assert!(executor.spawn(async { panic!() }).await.is_err());
    assert!(executor.spawn(async { "Real async tasks don't panic" }).await.is_ok());
}
