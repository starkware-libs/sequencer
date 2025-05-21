use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::Waker;

use waker_fn::waker_fn;

pub(super) struct MockWakerWrapper {
    times_woken: Arc<AtomicUsize>,
    waker: Waker,
}

impl MockWakerWrapper {
    pub fn new() -> Self {
        let times_woken = Arc::new(AtomicUsize::new(0));
        let times_woken_clone = times_woken.clone();
        let waker = waker_fn(move || {
            times_woken_clone.fetch_add(1, Ordering::SeqCst);
        });
        Self { times_woken, waker }
    }

    pub fn get_waker(&self) -> &Waker {
        &self.waker
    }

    pub fn times_woken(&self) -> usize {
        self.times_woken.load(Ordering::SeqCst)
    }
}
