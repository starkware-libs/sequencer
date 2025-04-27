use std::task::Waker;

/// A manager for handling wakers and waking them for some events.
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

    /// calls wake on the wakers and clears the list.
    /// Returns Ok(()) if there were wakers to wake, Err(()) if there were none.
    pub fn wake(&mut self) -> Result<(), ()> {
        if self.wakers.is_empty() {
            return Err(());
        }

        for waker in self.wakers.iter() {
            waker.wake_by_ref();
        }
        self.wakers.clear();
        Ok(())
    }
}
