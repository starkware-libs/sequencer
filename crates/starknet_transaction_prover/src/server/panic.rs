//! Process-wide panic hook: one structured `tracing` event with location +
//! backtrace (indexable by log aggregators) instead of the default ad-hoc
//! stderr output. The hook only logs: it does not change unwinding behavior, so
//! a panic inside a request task stays contained by the tokio runtime and the
//! process keeps serving.

use std::backtrace::Backtrace;
use std::panic::PanicHookInfo;

use tracing::error;

#[cfg(test)]
#[path = "panic_test.rs"]
mod panic_test;

pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(panic_hook));
}

fn panic_hook(info: &PanicHookInfo<'_>) {
    let payload = extract_payload(info);
    let location = info
        .location()
        .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
        .unwrap_or_else(|| "<unknown>".to_string());
    let backtrace = Backtrace::force_capture();
    error!(
        event = "panic",
        location = %location,
        payload = %payload,
        backtrace = %backtrace,
        "Service panicked",
    );
}

/// Only `&'static str` payloads (plain literals) are logged verbatim. `String`
/// payloads come from runtime formatting and may contain request or
/// transaction data, so they're redacted instead.
// TODO(Avi): Switch to PanicHookInfo::payload_as_str once it stabilizes.
fn extract_payload(info: &PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    if let Some(literal) = payload.downcast_ref::<&'static str>() {
        return (*literal).to_string();
    }
    if payload.downcast_ref::<String>().is_some() {
        return "<dynamic panic payload, redacted>".to_string();
    }
    "<non-string panic payload>".to_string()
}
