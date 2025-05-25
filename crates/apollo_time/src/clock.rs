use std::ops::Add;
use std::time::Duration;

/// Provides an `Instant` type for relative timing operations (deadlines, intervals).
/// The associated `Instant` type will likely be a `std::time::Instant` or a `tokio::time::Instant`,
/// see individual implementations for details.
pub trait InstantClock: Send + Sync {
    type Instant: Copy + Add<Duration, Output = Self::Instant>;
    fn now(&self) -> Self::Instant;
}
