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
    let payload = extract_payload(info);
    let location = info
        .location()
        .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
        .unwrap_or_else(|| "<unknown>".to_string());
    let backtrace = Backtrace::force_capture();
    // Field name must not be `message`: tracing reserves that name for the trailing
    // "Service panicked" literal below, and a duplicate `message` field collides with
    // it — the payload renders unprefixed with no `message=` key, unparseable by log
    // aggregators (caught by logs_structured_event_with_location_payload_and_backtrace).
    error!(
        event = "panic",
        location = %location,
        payload = %payload,
        backtrace = %backtrace,
        "Service panicked",
    );
}

/// Best-effort payload extraction. Only `&'static str` payloads (from
/// `panic!("literal")` with no interpolation) are logged verbatim — that text
/// is baked into the binary at compile time, so it's already reviewed source,
/// never runtime data. `String` payloads are heap-allocated at panic time
/// (e.g. `panic!("{}", value)`, or `Result::unwrap()`'s `Err` interpolation)
/// and can embed arbitrary runtime data, so they're redacted: this hook is
/// process-wide and catches panics from every dependency, not just this
/// crate's own reviewed call sites, so it can't assume an interpolated
/// payload is free of request or transaction content. Replace with
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
