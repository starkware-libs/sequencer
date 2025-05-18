use std::ops::Add;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Provides an `Instant` type for relative timing operations (deadlines, intervals).
/// The associated `Instant` type will likely be a `std::time::Instant` or a `tokio::time::Instant`,
/// see individual implementations for details.
pub trait InstantClock: Send + Sync {
    type Instant: Copy + Add<Duration, Output = Self::Instant>;
    fn now(&self) -> Self::Instant;
}

/// UnixClock provides absolute wall-clock time since the UNIX epoch.
/// Use `unix_now()` for subsecond durations and `unix_now_secs()` for whole seconds.
/// When the `chrono` feature is enabled, `chrono_unix_now()` returns a `DateTime<Utc>`.
pub trait UnixClock {
    fn unix_now(&self) -> Duration {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap()
    }

    fn unix_now_secs(&self) -> u64 {
        self.unix_now().as_secs()
    }

    // Legacy: we are using chrono in some places just to use unix time, these places can all be
    // replaced to use unix time method above and save the extra dependency.
    #[cfg(feature = "chrono")]
    fn chrono_unix_now(&self) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::<chrono::Utc>::from(UNIX_EPOCH + self.unix_now())
    }
}

pub trait Clock: InstantClock + UnixClock {}
impl<T: InstantClock + UnixClock> Clock for T {}
