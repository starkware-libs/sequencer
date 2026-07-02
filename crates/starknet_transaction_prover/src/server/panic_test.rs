use std::sync::{Arc, Mutex};

use crate::server::panic::extract_payload;

fn capture_payload<F: FnOnce() + std::panic::UnwindSafe>(f: F) -> String {
    let captured: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let test_thread_id = std::thread::current().id();
    let prev_hook = std::panic::take_hook();
    let writer = Arc::clone(&captured);
    std::panic::set_hook(Box::new(move |info| {
        // The hook is process-global; ignore panics from concurrently running
        // tests (e.g. `#[should_panic]` ones) so they can't pollute the capture.
        if std::thread::current().id() == test_thread_id {
            *writer.lock().unwrap() = Some(extract_payload(info));
        }
    }));
    let _ = std::panic::catch_unwind(f);
    std::panic::set_hook(prev_hook);
    // Bind first: the guard temporary must drop before `captured` does.
    let payload = captured.lock().unwrap().clone().unwrap_or_default();
    payload
}

// The panic hook is global state — a single `#[test]` keeps captures serial.
#[test]
fn extracts_static_str_and_formatted_payloads() {
    assert_eq!(capture_payload(|| panic!("static literal")), "static literal");
    assert_eq!(capture_payload(|| panic!("formatted {}", 42)), "formatted 42");
}
