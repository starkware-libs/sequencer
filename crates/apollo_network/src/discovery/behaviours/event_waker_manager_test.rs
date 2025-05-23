use super::EventWakerManager;
use crate::discovery::behaviours::test_util::MockWakerWrapper;

#[test]
fn no_wakers_added() {
    let mut waker_manager = EventWakerManager::default();
    waker_manager.wake();
}

#[test]
fn wakes_all_wakers() {
    let waker_wrapper1 = MockWakerWrapper::new();
    let waker_wrapper2 = MockWakerWrapper::new();

    let mut waker_manager = EventWakerManager::default();
    waker_manager.add_waker(waker_wrapper1.get_waker());
    waker_manager.add_waker(waker_wrapper2.get_waker());

    assert_eq!(waker_wrapper1.times_woken(), 0);
    assert_eq!(waker_wrapper2.times_woken(), 0);

    waker_manager.wake();

    assert_eq!(waker_wrapper1.times_woken(), 1);
    assert_eq!(waker_wrapper2.times_woken(), 1);
}

#[test]
fn does_not_wake_waker_twice() {
    let waker_wrapper = MockWakerWrapper::new();

    let mut waker_manager = EventWakerManager::default();
    waker_manager.add_waker(waker_wrapper.get_waker());

    waker_manager.wake();
    assert_eq!(waker_wrapper.times_woken(), 1);

    waker_manager.wake();
    assert_eq!(waker_wrapper.times_woken(), 1);
}
