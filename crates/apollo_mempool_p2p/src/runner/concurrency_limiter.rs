use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

pub(super) struct ConcurrencyLimiter {
    current: Arc<AtomicUsize>,
    max: usize,
}

pub(super) struct ConcurrencyLimiterGuard {
    current: Arc<AtomicUsize>,
}

impl ConcurrencyLimiter {
    pub fn new(max: usize) -> Self {
        Self { current: Arc::new(AtomicUsize::new(0)), max }
    }

    pub fn try_acquire(&self) -> Option<ConcurrencyLimiterGuard> {
        let current = self.current.load(Ordering::Relaxed);
        if current >= self.max {
            return None;
        }
        self.current.fetch_add(1, Ordering::Relaxed);
        Some(ConcurrencyLimiterGuard { current: self.current.clone() })
    }
}

impl Drop for ConcurrencyLimiterGuard {
    fn drop(&mut self) {
        self.current.fetch_sub(1, Ordering::Relaxed);
    }
}
