use tokio::time::Instant as TokioInstant;

use crate::clock::{InstantClock, UnixClock};

/// Clock that relies on the tokio runtime for relative timing, which wraps STD `Instant` and allows
/// for better global control during tests, like `pause` and `advance`, which halts the clock
/// globally.
#[derive(Debug, Default)]
pub struct TokioClock;

impl InstantClock for TokioClock {
    type Instant = TokioInstant;
    fn now(&self) -> TokioInstant {
        TokioInstant::now()
    }
}

impl UnixClock for TokioClock {}
