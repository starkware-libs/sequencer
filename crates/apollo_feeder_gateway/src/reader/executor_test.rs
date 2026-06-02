use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::reader::executor::ReadExecutor;

#[tokio::test]
async fn run_respects_concurrency_bound_and_completes_all() {
    const READ_POOL_SIZE: usize = 4;
    const NUM_TASKS: usize = 32;

    let executor = Arc::new(ReadExecutor::new(READ_POOL_SIZE));
    let in_flight = Arc::new(AtomicUsize::new(0));
    let max_in_flight = Arc::new(AtomicUsize::new(0));
    let completed = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for _ in 0..NUM_TASKS {
        let executor = executor.clone();
        let in_flight = in_flight.clone();
        let max_in_flight = max_in_flight.clone();
        let completed = completed.clone();
        handles.push(tokio::spawn(async move {
            executor
                .run(move || {
                    let current = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                    max_in_flight.fetch_max(current, Ordering::SeqCst);
                    // Hold the permit long enough that excess tasks must queue.
                    std::thread::sleep(Duration::from_millis(10));
                    in_flight.fetch_sub(1, Ordering::SeqCst);
                })
                .await
                .unwrap();
            completed.fetch_add(1, Ordering::SeqCst);
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // Every task completed (no rejection under saturation).
    assert_eq!(completed.load(Ordering::SeqCst), NUM_TASKS);
    assert_eq!(in_flight.load(Ordering::SeqCst), 0);
    // Concurrency never exceeded the bound.
    let observed_max = max_in_flight.load(Ordering::SeqCst);
    assert!(
        observed_max <= READ_POOL_SIZE,
        "observed {observed_max} concurrent reads, bound is {READ_POOL_SIZE}"
    );
}
