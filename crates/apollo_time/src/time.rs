use std::fmt::Debug;

pub type DateTime = chrono::DateTime<chrono::Utc>;

// TODO(Gilad): add fake clock with fixed time + advance(), instead of mockall, easier to use.
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
pub trait Clock: Send + Sync + Debug {
    /// Human readable representation of unix time (uses duration from epoch under the hood).
    // Note: chrono is used here since it wraps unix time and allows it to be printed in datetime
    // format, since it is otherwise not human readable.
    fn now(&self) -> DateTime {
        chrono::Utc::now()
    }

    /// Seconds from epoch.
    fn unix_now(&self) -> u64 {
        self.now().timestamp().try_into().expect("We shouldn't have dates before the unix epoch")
    }
}

pub trait ClockExt: Clock {
    #[cfg(feature = "tokio")]
    fn sleep_until<'a>(
        &'a self,
        deadline: DateTime,
    ) -> impl core::future::Future<Output = ()> + Send + 'a {
        async move {
            // Calculate how much time is left until the deadline.
            // If the deadline has already passed, this will be a negative duration.
            let time_delta = deadline - self.now();
            // Convert to `std::time::Duration`, clamping any negative value to zero.
            // A zero-duration sleep is effectively a no-op.
            let duration_to_sleep = time_delta.to_std().unwrap_or_default();
            // Sleep for the computed duration (or return immediately if zero).
            tokio::time::sleep(duration_to_sleep).await;
        }
    }
}

impl<T: Clock + ?Sized> ClockExt for T {}

#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultClock;

impl Clock for DefaultClock {}
