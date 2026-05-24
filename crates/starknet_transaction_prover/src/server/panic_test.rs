use std::sync::{Arc, Mutex};

use crate::server::panic::extract_payload;

fn capture_payload<F: FnOnce() + std::panic::UnwindSafe>(f: F) -> String {
    let captured: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let prev_hook = std::panic::take_hook();
    let writer = Arc::clone(&captured);
    std::panic::set_hook(Box::new(move |info| {
        *writer.lock().unwrap() = Some(extract_payload(info));
    }));
    let _ = std::panic::catch_unwind(f);
    std::panic::set_hook(prev_hook);
    let value = captured.lock().unwrap().clone().unwrap_or_default();
    value
}

// Panic-capturing tests share global state (the panic hook), so they must
// run serially. Keep as a single `#[test]` so ordering is explicit.
#[test]
fn extracts_static_str_and_formatted_payloads() {
    assert_eq!(capture_payload(|| panic!("static literal")), "static literal");
    assert_eq!(capture_payload(|| panic!("formatted {}", 42)), "formatted 42");
}
