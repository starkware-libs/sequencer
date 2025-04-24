use std::task::Context;

use tokio::time::Instant;

pub mod bootstrapping;
pub mod kad_requesting;

/// Function that sets up the waker of the context to wake up at a specific instant.
pub fn configure_context_to_wake_at_instant(cx: &mut Context<'_>, instant: Instant) {
    let waker = cx.waker().clone();
    let future = async move {
        tokio::time::sleep_until(instant).await;
        waker.wake();
    };
    tokio::spawn(future);
}
