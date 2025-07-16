use std::fmt::Debug;

use async_trait::async_trait;

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

#[cfg(feature = "tokio")]
#[async_trait]
pub trait ClockExt: Clock + Send + Sync {
    async fn sleep_until(&self, deadline: DateTime) {
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

// Testing requires a struct that implements both Clock and ClockExt.
#[cfg(any(test, feature = "testing"))]
mockall::mock! {
    #[derive(Debug)]
    pub TestClock {}

    impl Clock for TestClock {
        fn now(&self) -> DateTime {
            self.now()
        }

        fn unix_now(&self) -> u64 {
            self.unix_now()
        }
    }

    #[async_trait]
    impl ClockExt for TestClock {
        async fn sleep_until(&self, deadline: DateTime) {
            self.sleep_until(deadline).await
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultClock;

impl Clock for DefaultClock {}

impl ClockExt for DefaultClock {}
