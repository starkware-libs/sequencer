//! Process-wide panic hook. It replaces the default stderr output with one
//! structured `tracing` event carrying the panic location and a backtrace,
//! which log aggregators can index. The hook only logs. It does not change
//! unwinding behavior, so the tokio runtime still contains a panic raised
//! inside a request task and the process keeps serving.

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

/// Returns `&'static str` payloads (plain literals) as they are. A `String`
/// payload comes from runtime formatting and may carry request or transaction
/// data, so this returns a placeholder for it instead.
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
