use std::sync::{Arc, Mutex};

use crate::server::metrics::names::PANICS_TOTAL;
use crate::server::panic::{extract_payload, install_panic_hook};
use crate::server::test_recorder::shared_handle;

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

// Panic-capturing tests share global state (the panic hook); both tests in
// this module install/restore the hook around a single `catch_unwind`.
#[test]
fn extracts_static_str_and_formatted_payloads() {
    assert_eq!(capture_payload(|| panic!("static literal")), "static literal");
    assert_eq!(capture_payload(|| panic!("formatted {}", 42)), "formatted 42");
}

#[test]
fn panic_hook_bumps_panics_total_counter() {
    let handle = shared_handle();
    let before = counter_value(&handle.render(), PANICS_TOTAL);

    let prev_hook = std::panic::take_hook();
    install_panic_hook();
    let _ = std::panic::catch_unwind(|| panic!("counter-test panic"));
    std::panic::set_hook(prev_hook);

    let after = counter_value(&handle.render(), PANICS_TOTAL);
    assert_eq!(after - before, 1.0);
}

fn counter_value(scrape: &str, name: &str) -> f64 {
    scrape
        .lines()
        .find(|line| line.starts_with(name) && !line.starts_with("# "))
        .and_then(|line| line.rsplit_once(' ').and_then(|(_, v)| v.parse().ok()))
        .unwrap_or(0.0)
}
