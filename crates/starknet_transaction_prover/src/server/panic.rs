//! Process-wide panic hook for the prover.
//!
//! Without an explicit hook, panics in `tokio::spawn`ed work hit the runtime's
//! default handler which prints to stderr in an ad-hoc format. We want one
//! structured `tracing` event with location + backtrace so log aggregators
//! (Datadog) can index it. The hook also bumps `prover_panics_total` so
//! dashboards/alerts can fire on panic rate rather than relying on log
//! search.
//!
//! The hook does *not* call `process::abort()` — the existing runtime
//! behavior (which aborts on unhandled task panic by default for
//! `#[tokio::main]`) is preserved.

use std::backtrace::Backtrace;
use std::panic::PanicHookInfo;

use tracing::error;

use super::metrics::names::PANICS_TOTAL;

pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(panic_hook));
}

fn panic_hook(info: &PanicHookInfo<'_>) {
    // Increment the panic counter *first* — if `Backtrace::force_capture`
    // or the `error!` macro itself were to panic recursively, the counter
    // would still reflect the original panic.
    metrics::counter!(PANICS_TOTAL).increment(1);
    let message = extract_payload(info);
    let location = info
        .location()
        .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
        .unwrap_or_else(|| "<unknown>".to_string());
    // `force_capture` to get a backtrace regardless of RUST_BACKTRACE.
    let backtrace = Backtrace::force_capture();
    error!(
        event = "panic",
        location = %location,
        message = %message,
        backtrace = %backtrace,
        "Service panicked",
    );
}

/// Best-effort extraction of the panic payload — supports the common
/// `panic!("string literal")` and `panic!("{fmt}", ...)` cases. Returns
/// `"<non-string panic payload>"` for arbitrary types.
fn extract_payload(info: &PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        return (*s).to_string();
    }
    if let Some(s) = payload.downcast_ref::<String>() {
        return s.clone();
    }
    "<non-string panic payload>".to_string()
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::extract_payload;

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
    // run serially. Using a shared mutex would force the same — keep these as
    // a single `#[test]` to make ordering explicit.
    #[test]
    fn extracts_static_str_and_formatted_payloads() {
        assert_eq!(capture_payload(|| panic!("static literal")), "static literal");
        assert_eq!(capture_payload(|| panic!("formatted {}", 42)), "formatted 42");
    }

    #[test]
    fn panic_hook_bumps_panics_total_counter() {
        use super::super::metrics::names::PANICS_TOTAL;
        use super::super::test_recorder::shared_handle;

        // Ensure the global Prometheus recorder is installed so the
        // `metrics::counter!` call inside the hook has a backing store.
        let handle = shared_handle();
        let before = counter_value(&handle.render(), PANICS_TOTAL);

        // Install the production hook (it does not exit; only logs +
        // bumps the counter), trigger a panic, then restore the previous
        // hook so we don't disturb the other test.
        let prev_hook = std::panic::take_hook();
        super::install_panic_hook();
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
}
