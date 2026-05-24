//! Process-wide panic hook: one structured `tracing` event with location +
//! backtrace (indexable by log aggregators) instead of the default ad-hoc
//! stderr output, plus a `prover_panics_total` bump so alerts can fire on
//! panic rate rather than log search. Only logs and counts — runtime
//! abort-on-panic behavior is preserved.

use std::backtrace::Backtrace;
use std::panic::PanicHookInfo;

use tracing::error;

use crate::server::metrics::names::PANICS_TOTAL;

#[cfg(test)]
#[path = "panic_test.rs"]
mod panic_test;

pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(panic_hook));
}

fn panic_hook(info: &PanicHookInfo<'_>) {
    // Increment first — if `Backtrace::force_capture` or the `error!` macro
    // panic recursively, the counter still reflects the original panic.
    metrics::counter!(PANICS_TOTAL).increment(1);
    let message = extract_payload(info);
    let location = info
        .location()
        .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
        .unwrap_or_else(|| "<unknown>".to_string());
    let backtrace = Backtrace::force_capture();
    error!(
        event = "panic",
        location = %location,
        message = %message,
        backtrace = %backtrace,
        "Service panicked",
    );
}

/// Best-effort payload extraction: `&'static str` and `String` payloads,
/// `"<non-string panic payload>"` otherwise. Replace with
/// `PanicHookInfo::payload_as_str()` once it stabilizes.
pub(crate) fn extract_payload(info: &PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        return (*s).to_string();
    }
    if let Some(s) = payload.downcast_ref::<String>() {
        return s.clone();
    }
    "<non-string panic payload>".to_string()
}
