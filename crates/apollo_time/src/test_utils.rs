use std::sync::Mutex;
use std::time::Duration;

use crate::time::{Clock, DateTime};

#[derive(Debug)]
pub struct FakeClock {
    pub offset: Mutex<Duration>,
    pub base_time: DateTime,
}

impl FakeClock {
    pub fn new(base_time: u64) -> Self {
        FakeClock {
            offset: Mutex::new(Duration::ZERO),
            base_time: chrono::DateTime::from_timestamp(base_time.try_into().unwrap(), 0).unwrap(),
        }
    }

    pub fn advance(&self, duration: Duration) {
        let mut off = self.offset.lock().unwrap();
        *off = off.saturating_add(duration);
    }
}

impl Clock for FakeClock {
    fn now(&self) -> DateTime {
        let off = *self.offset.lock().unwrap();
        self.base_time + off
    }
}

impl Default for FakeClock {
    fn default() -> Self {
        FakeClock { offset: Mutex::new(Duration::ZERO), base_time: chrono::Utc::now() }
    }
}
