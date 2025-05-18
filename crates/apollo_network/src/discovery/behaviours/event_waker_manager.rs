// TODO(AndrewL): extract to a util file and use it in sqmr
// TODO(AndrewL): add tests for this file
use std::task::Waker;

/// A manager which handles waking multiple wakers.
#[derive(Default)]
pub(super) struct EventWakerManager {
    wakers: Vec<Waker>,
}

impl EventWakerManager {
    /// Adds a waker to the list of wakers.
    /// Returns true if the waker was added, false if it was already present.
    pub fn add_waker(&mut self, waker: &Waker) -> bool {
        if self.wakers.iter().any(|w| w.will_wake(waker)) {
            return false;
        }
        self.wakers.push(waker.clone());
        true
    }

    /// Calls wake on the wakers and clears the list.
    /// Returns true if there were wakers to wake, false if there were none.
    pub fn wake(&mut self) {
        for waker in self.wakers.iter() {
            waker.wake_by_ref();
        }
        self.wakers.clear();
    }
}
