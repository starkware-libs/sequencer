use std::ops::Deref;
use std::sync::Arc;

use async_trait::async_trait;

// TODO(Gilad): add fake clock with fixed time + advance(), instead of mockall, easier to use.
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
pub trait Clock: Send + Sync {
    /// Human readable representation of unix time (uses duration from epoch under the hood).
    // Note: chrono is used here since it wraps unix time and allows it to be printed in datetime
    // format, since it is otherwise not human readable.
    fn now(&self) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc::now()
    }

    /// Seconds from epoch.
    fn unix_now(&self) -> u64 {
        self.now().timestamp().try_into().expect("We shouldn't have dates before the unix epoch")
    }
}

/// Contains sleep logic, in order to decouple from std/tokio constraints.
// Consider adding a wrapper around tokio sleep, to decouple from global tokio sleep.
#[async_trait]
pub trait Sleeper: Send + Sync {
    async fn sleep_until(&self, deadline: chrono::DateTime<chrono::Utc>);
}

#[derive(Clone, Default)]
pub struct DefaultClock();

impl Clock for DefaultClock {}

#[derive(Clone)]
pub struct TimeKeeper {
    pub clock: Arc<dyn Clock>,
}

#[async_trait]
impl Sleeper for TimeKeeper {
    // From Tokio maintainer: https://github.com/tokio-rs/tokio/issues/3918#issuecomment-896192957.
    async fn sleep_until(&self, deadline: chrono::DateTime<chrono::Utc>) {
        // Duration until deadline or `Duration::ZERO` if deadline has passed.
        let duration =
            deadline.signed_duration_since(self.clock.now()).to_std().unwrap_or_default();
        // Note: this is a NOP on `Duration::ZERO`.
        tokio::time::sleep(duration).await;
    }
}

impl Deref for TimeKeeper {
    type Target = dyn Clock;
    fn deref(&self) -> &Self::Target {
        self.clock.as_ref()
    }
}

impl Default for TimeKeeper {
    fn default() -> Self {
        Self { clock: Arc::new(DefaultClock::default()) }
    }
}
