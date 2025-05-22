use std::ops::Add;
use std::sync::Mutex;
use std::time::{Duration, Instant as StdInstant};

use crate::clock::InstantClock;

#[derive(Debug)]
pub struct FakeClock<I: Copy + Add<Duration, Output = I> + Send + Sync> {
    offset: Mutex<Duration>,
    base_instant: I,
}

impl<I: Copy + Add<Duration, Output = I> + Send + Sync> FakeClock<I> {
    pub fn new(base_instant: I) -> Self {
        FakeClock { offset: Mutex::new(Duration::ZERO), base_instant }
    }

    pub fn advance(&self, duration: Duration) {
        let mut off = self.offset.lock().unwrap();
        *off = off.saturating_add(duration);
    }
}

impl<I: Copy + Add<Duration, Output = I> + Send + Sync> InstantClock for FakeClock<I>
where
    I: Copy + Add<Duration, Output = I> + Send + Sync,
{
    type Instant = I;

    fn now(&self) -> I {
        let off = *self.offset.lock().unwrap();
        self.base_instant + off
    }
}

impl Default for FakeClock<StdInstant> {
    fn default() -> Self {
        FakeClock { offset: Mutex::new(Duration::ZERO), base_instant: StdInstant::now() }
    }
}
