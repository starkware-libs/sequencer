//! Test helper binary for verifying actual log output from configure_tracing().
//! This is invoked as a subprocess by trace_util_tests to capture real stdout.

use apollo_infra::trace_util::configure_tracing;
use thiserror::Error;
use tracing::instrument;

#[derive(Debug, Error)]
#[error("{0}")]
struct TestError(&'static str);

#[instrument(err)]
fn failing_function() -> Result<(), TestError> {
    Err(TestError("something went wrong"))
}

#[tokio::main]
async fn main() {
    configure_tracing().await;
    let _ = failing_function();
}
