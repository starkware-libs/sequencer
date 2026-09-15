//! Capture diagnostics at the synchronous prover boundary using the service's panic hook.

use std::any::Any;
use std::panic::{self, AssertUnwindSafe};

use crate::errors::PanicLocation;
use crate::server::panic::take_last_panic_location;

#[cfg(test)]
#[path = "panic_capture_test.rs"]
mod panic_capture_test;

#[derive(Debug)]
pub(super) struct ProverPanic {
    pub location: Option<PanicLocation>,
    pub message: String,
}

/// Without the service hook, or for a resumed panic, the location may be absent.
pub(super) fn catch_panic_with_location<T>(
    operation: impl FnOnce() -> T,
) -> Result<T, ProverPanic> {
    // Do not attribute an earlier panic on this thread to this operation.
    take_last_panic_location();
    let result = panic::catch_unwind(AssertUnwindSafe(operation));
    let location = take_last_panic_location();
    result.map_err(|payload| ProverPanic { location, message: panic_message(payload) })
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else {
        "non-string panic payload".to_string()
    }
}
