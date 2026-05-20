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
    let message = info.payload_as_str().unwrap_or("<non-string panic payload>");
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
