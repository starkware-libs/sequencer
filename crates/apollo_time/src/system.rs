use std::time::Instant as StdInstant;

use crate::clock::{InstantClock, UnixClock};

/// A clock that relies on the the internal OS clock (CLOCK_MONOTONIC) for relative timing.
/// Use this when you need real-world timing without interfacing with tokio Instance (don't mix the
/// two in a single flow, as this violates some assumptions tokio makes on timing).
pub struct SystemClock;

impl InstantClock for SystemClock {
    type Instant = StdInstant;

    fn now(&self) -> Self::Instant {
        StdInstant::now()
    }
}

impl UnixClock for SystemClock {}
