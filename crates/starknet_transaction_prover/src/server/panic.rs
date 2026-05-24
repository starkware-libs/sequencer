//! Process-wide panic hook for the prover.
//!
//! Without an explicit hook, panics in `tokio::spawn`ed work hit the runtime's
//! default handler which prints to stderr in an ad-hoc format. We want one
//! structured `tracing` event with location + backtrace so log aggregators
//! (Datadog) can index it and on-call can be paged.
//!
//! The hook only emits a log line. It does *not* call `process::abort()` —
//! the existing runtime behavior (which aborts on unhandled task panic by
//! default for `#[tokio::main]`) is preserved.

use std::backtrace::Backtrace;
use std::panic::PanicHookInfo;

use tracing::error;

pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(panic_hook));
}

fn panic_hook(info: &PanicHookInfo<'_>) {
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
///
/// Could be replaced by `PanicHookInfo::payload_as_str()` once the prover's
/// pinned toolchain (`nightly-2025-07-14`) ships it as stable — gated behind
/// `panic_payload_as_str` today.
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
    // run serially. Keep these as a single `#[test]` so ordering is explicit.
    #[test]
    fn extracts_static_str_and_formatted_payloads() {
        assert_eq!(capture_payload(|| panic!("static literal")), "static literal");
        assert_eq!(capture_payload(|| panic!("formatted {}", 42)), "formatted 42");
    }
}
