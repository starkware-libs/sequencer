use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use futures::future::BoxFuture;
use futures::FutureExt;
use tokio::time::Instant;

pub mod bootstrapping;
pub mod kad_requesting;

const LOCK_ERROR: &str = "Failed to lock waker list.";

/// A manager for handling wakers and scheduling them to wake them at specific times.
#[derive(Default)]
struct TimeWakerManager {
    wakers: Arc<Mutex<Vec<Waker>>>,
    last_timer: Option<(BoxFuture<'static, ()>, Instant)>,
}

impl TimeWakerManager {
    /// Add a waker that will be used in the next wake.
    /// Should likely be called at the start of a `poll` function
    ///
    /// Returns true if the waker was added, false if a waker for the same task was already added.
    pub fn add_waker(&mut self, waker: &Waker) -> bool {
        let mut locked_wakers = self.wakers.lock().expect(LOCK_ERROR);
        if locked_wakers.iter().any(|w| w.will_wake(waker)) {
            return false;
        }
        locked_wakers.push(waker.clone());
        true
    }

    /// Spawns a task that will wake the waker at a specific instant.
    pub fn wake_at(&mut self, cx: &mut Context<'_>, instant: Instant) -> Poll<()> {
        self.add_waker(cx.waker());

        let should_update_timer = if let Some((_, last_instant)) = self.last_timer.as_ref() {
            *last_instant > instant
        } else {
            true
        };

        if should_update_timer {
            let wakers = Arc::clone(&self.wakers);
            self.last_timer = Some((
                Box::pin(async move {
                    tokio::time::sleep_until(instant).await;
                    Self::wake_aux(&wakers);
                }),
                instant,
            ));
        }

        let poll_result = self.last_timer.as_mut().unwrap().0.poll_unpin(cx);

        match poll_result {
            Poll::Ready(_) => {
                self.last_timer = None;
                Poll::Ready(())
            }
            Poll::Pending => Poll::Pending,
        }
    }

    /// calls wake on the waker.
    pub fn wake(&mut self) {
        Self::wake_aux(&self.wakers);
    }

    /// Function that wakes all wakers in the list and clears the list.
    fn wake_aux(wakers: &Arc<Mutex<Vec<Waker>>>) {
        let mut locked_wakers = wakers.lock().expect(LOCK_ERROR);
        for waker in locked_wakers.iter() {
            waker.wake_by_ref();
        }
        locked_wakers.clear();
    }
}
