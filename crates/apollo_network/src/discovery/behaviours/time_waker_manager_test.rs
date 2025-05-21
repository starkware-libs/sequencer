use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use futures::FutureExt;
use tokio::time::{sleep, Duration, Instant};
use waker_fn::waker_fn;

use crate::discovery::behaviours::test_util::MockWakerWrapper;
use crate::discovery::behaviours::TimeWakerManager;

#[tokio::test(start_paused = true)]
async fn wakes_waker_after_time() {
    const TIME_TO_WAIT: Duration = Duration::from_millis(100);

    let start = Instant::now();

    let waker = MockWakerWrapper::new();

    let mut time_wake_manager = TimeWakerManager::default();
    let mut cx = waker.create_context();

    let wake_time = start.checked_add(TIME_TO_WAIT).unwrap();

    time_wake_manager.wake_at(&mut cx, wake_time);

    // Advance time to just before the desired wake time and make sure the waker wasn't called.
    const TIME_DELTA: Duration = Duration::from_millis(1);
    tokio::time::advance(TIME_TO_WAIT - TIME_DELTA).await;

    let _ = time_wake_manager.poll_unpin(&mut cx);
}
