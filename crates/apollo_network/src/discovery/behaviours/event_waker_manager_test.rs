use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use waker_fn::waker_fn;

use super::EventWakerManager;

#[test]
fn no_wakers_added() {
    let mut waker_manager = EventWakerManager::default();
    waker_manager.wake();
}

#[test]
fn wakes_all_wakers() {
    let waker_1_was_called = Arc::new(AtomicBool::new(false));
    let waker_1_was_called_clone = waker_1_was_called.clone();
    let waker1 = waker_fn(move || {
        waker_1_was_called_clone.store(true, Ordering::SeqCst);
    });

    let waker_2_was_called = Arc::new(AtomicBool::new(false));
    let waker_2_was_called_clone = waker_2_was_called.clone();
    let waker2 = waker_fn(move || {
        waker_2_was_called_clone.store(true, Ordering::SeqCst);
    });

    let mut waker_manager = EventWakerManager::default();
    waker_manager.add_waker(&waker1);
    waker_manager.add_waker(&waker2);

    assert!(!waker_1_was_called.load(Ordering::SeqCst));
    assert!(!waker_2_was_called.load(Ordering::SeqCst));

    waker_manager.wake();

    assert!(waker_1_was_called.load(Ordering::SeqCst));
    assert!(waker_2_was_called.load(Ordering::SeqCst));
}

#[test]
fn does_not_wake_waker_twice() {
    let waker_was_called = Arc::new(AtomicBool::new(false));
    let waker_was_called_clone = waker_was_called.clone();
    let waker = waker_fn(move || {
        waker_was_called_clone.store(true, Ordering::SeqCst);
    });

    let mut waker_manager = EventWakerManager::default();
    waker_manager.add_waker(&waker);

    waker_manager.wake();

    assert!(waker_was_called.load(Ordering::SeqCst));
    // Set the value back to false to see it doesn't get set to true by the waker call.
    waker_was_called.store(false, Ordering::SeqCst);

    waker_manager.wake();
    assert!(!waker_was_called.load(Ordering::SeqCst));
}
