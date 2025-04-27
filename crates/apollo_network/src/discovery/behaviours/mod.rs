use std::task::{Context, Poll, Waker};

use futures::FutureExt;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tracing::error;

pub mod bootstrapping;
pub mod kad_requesting;

#[derive(Debug, Default)]
pub struct TimeWakerManager {
    current_waker: Option<Waker>,
    join_handles: Vec<JoinHandle<()>>,
}

impl TimeWakerManager {
    /// Set the most recent waker that will be used to wake.
    /// * Overrides the last set waker
    /// * Should likely be called at the start of a `poll` function
    /// * **Aborts previous wake timers**
    pub fn set_waker(&mut self, waker: Waker) {
        self.current_waker = Some(waker);

        for handle in self.join_handles.iter_mut() {
            let mut cx = Context::from_waker(Waker::noop());
            match handle.poll_unpin(&mut cx) {
                Poll::Ready(r) => r.expect("Deployed timing future failed"),
                Poll::Pending => {
                    handle.abort();
                }
            }
        }
        self.join_handles.clear();
    }

    /// Spawns a task that will wake the waker at a specific instant
    pub fn wake_at(&mut self, instant: Instant) {
        if let Some(waker) = &self.current_waker {
            let waker = waker.clone();
            let timing_future = async move {
                tokio::time::sleep_until(instant).await;
                waker.wake();
            };
            let handle = tokio::spawn(timing_future);
            self.join_handles.push(handle);
        } else {
            // This should never happen
            error!("Attempted waking when no waker exists!")
        }
    }

    /// calls wake on the waker
    pub fn wake_now(&mut self) {
        if let Some(waker) = &self.current_waker {
            waker.wake_by_ref();
        } else {
            // This should never happen
            error!("Attempted waking when no waker exists!")
        }
    }
}
