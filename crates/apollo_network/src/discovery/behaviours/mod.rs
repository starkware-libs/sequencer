use std::task::Waker;

use tokio::task::JoinHandle;
use tokio::time::Instant;

pub mod bootstrapping;
pub mod kad_requesting;

#[derive(Debug, Default)]
pub struct TimeWakerManager {
    wakers: Vec<Waker>,
    join_handles: Vec<JoinHandle<()>>,
}

impl TimeWakerManager {
    /// Set the most recent waker that will be used to wake.
    /// * Overrides the last set waker
    /// * Should likely be called at the start of a `poll` function
    /// * **Aborts previous wake timers**
    pub fn add_waker(&mut self, waker: &Waker) -> bool {
        if self.wakers.iter().any(|w| w.will_wake(waker)) {
            return false;
        }

        self.join_handles.retain(|handle| !handle.is_finished());
        self.wakers.push(waker.clone());
        true
    }

    /// Spawns a task that will wake the waker at a specific instant
    ///
    /// Returns an error if no waker was added.
    pub fn wake_at(&mut self, instant: Instant) -> Result<(), ()> {
        if self.wakers.is_empty() {
            return Err(());
        };

        let wakers = self.wakers.clone();
        let timing_future = async move {
            tokio::time::sleep_until(instant).await;
            for waker in wakers {
                waker.wake();
            }
        };
        let handle = tokio::spawn(timing_future);
        self.join_handles.push(handle);
        Ok(())
    }

    /// calls wake on the waker.
    ///
    /// Returns an error if no waker was added.
    pub fn wake(&mut self) -> Result<(), ()> {
        if self.wakers.is_empty() {
            return Err(());
        };
        for waker in self.wakers.iter() {
            waker.wake_by_ref();
        }
        Ok(())
    }
}
