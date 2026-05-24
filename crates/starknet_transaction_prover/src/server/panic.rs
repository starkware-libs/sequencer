//! Process-wide panic hook for the prover.
//!
//! Without an explicit hook, panics in `tokio::spawn`ed work hit the runtime's
//! default handler and print to stderr in an ad-hoc format. We want one
//! structured `tracing` event with location + backtrace so log aggregators
//! can index it. The hook only emits a log line — runtime abort-on-panic
//! behavior is preserved.

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

/// Best-effort extraction of the panic payload — supports the common
/// `panic!("string literal")` and `panic!("{fmt}", ...)` cases. Returns
/// `"<non-string panic payload>"` for arbitrary types.
///
/// Replace with `PanicHookInfo::payload_as_str()` once the crate's pinned nightly
/// (see `rust-toolchain.toml`) stabilizes it (currently gated behind the
/// `panic_payload_as_str` feature).
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
