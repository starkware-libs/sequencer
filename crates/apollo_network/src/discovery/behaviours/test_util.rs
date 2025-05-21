use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Waker};

use waker_fn::waker_fn;

const SYNC_ATOMIC_ORDERING: Ordering = Ordering::SeqCst;

pub(super) struct MockWakerWrapper {
    times_woken: Arc<AtomicUsize>,
    waker: Waker,
}

impl MockWakerWrapper {
    pub fn new() -> Self {
        let times_woken = Arc::new(AtomicUsize::new(0));
        let times_woken_clone = times_woken.clone();
        let waker = waker_fn(move || {
            times_woken_clone.fetch_add(1, SYNC_ATOMIC_ORDERING);
        });
        Self { times_woken, waker }
    }

    pub fn get_waker(&self) -> &Waker {
        &self.waker
    }

    pub fn create_context(&self) -> Context {
        Context::from_waker(&self.waker)
    }

    pub fn times_woken(&self) -> usize {
        self.times_woken.load(SYNC_ATOMIC_ORDERING)
    }
}
