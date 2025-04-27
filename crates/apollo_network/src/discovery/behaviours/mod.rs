use std::task::Waker;

use tokio::time::Instant;

pub mod bootstrapping;
pub mod kad_requesting;

/// Function that sets up the waker of the context to wake up at a specific instant.
pub fn configure_context_to_wake_at_instant(waker: Waker, instant: Instant) {
    let future = async move {
        tokio::time::sleep_until(instant).await;
        waker.wake();
    };
    tokio::spawn(future);
}
