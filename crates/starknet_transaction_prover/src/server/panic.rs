//! Process-wide panic hook: one structured `tracing` event with location +
//! backtrace (indexable by log aggregators) and a `prover_panics_total` bump,
//! instead of the default ad-hoc stderr output. Only logs and counts —
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
    // Increment first so a recursive panic in the logging below can't lose the count.
    metrics::counter!(PANICS_TOTAL).increment(1);
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
/// transaction data, so they're redacted instead. Replace with
/// `PanicHookInfo::payload_as_str()` once it stabilizes.
pub(crate) fn extract_payload(info: &PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        return (*s).to_string();
    }
    if payload.downcast_ref::<String>().is_some() {
        return "<dynamic panic payload, redacted>".to_string();
    }
    "<non-string panic payload>".to_string()
}
