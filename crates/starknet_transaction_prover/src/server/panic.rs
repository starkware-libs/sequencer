//! Process-wide panic hook. It replaces the default stderr output with one
//! structured `tracing` event carrying the panic location and a backtrace,
//! which log aggregators can index. The hook does not change
//! unwinding behavior, so the tokio runtime still contains a panic raised
//! inside a request task and the process keeps serving.
//!
//! The hook also records the panic's location on the panicking thread, so proving code can
//! attribute a caught panic to its source.

use std::backtrace::Backtrace;
use std::cell::RefCell;
use std::panic::PanicHookInfo;

use tracing::error;

use crate::errors::PanicLocation;

#[cfg(test)]
#[path = "panic_test.rs"]
mod panic_test;

thread_local! {
    /// Source location of the most recent panic on this thread, recorded by [`panic_hook`].
    static LAST_PANIC_LOCATION: RefCell<Option<PanicLocation>> = const { RefCell::new(None) };
}

pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(panic_hook));
}

/// Returns and clears the source location of the most recent panic recorded on this thread by
/// [`panic_hook`]. `None` if no panic has been recorded on this thread since the last call (or
/// since the hook was installed, if this is the first call).
#[cfg(any(test, feature = "stwo_proving"))]
pub(crate) fn take_last_panic_location() -> Option<PanicLocation> {
    LAST_PANIC_LOCATION.take()
}

fn panic_hook(info: &PanicHookInfo<'_>) {
    let payload = extract_payload(info);
    let location = info.location();
    // Thread-local storage may already be destroyed when a thread panics during teardown.
    let _ = LAST_PANIC_LOCATION.try_with(|slot| {
        if let Ok(mut slot) = slot.try_borrow_mut() {
            *slot = location
                .map(|loc| PanicLocation { file: loc.file().to_string(), line: loc.line() });
        }
    });
    let location_display = location
        .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
        .unwrap_or_else(|| "<unknown>".to_string());
    let backtrace = Backtrace::force_capture();
    error!(
        event = "panic",
        location = %location_display,
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
