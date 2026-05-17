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
    // Increment first — if `Backtrace::force_capture` or the `error!` macro
    // panic recursively, the counter still reflects the original panic.
    metrics::counter!(PANICS_TOTAL).increment(1);
    let message = info.payload_as_str().unwrap_or("<non-string panic payload>");
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

#[cfg(test)]
mod tests {
    use super::super::metrics::names::PANICS_TOTAL;
    use super::super::test_recorder::shared_handle;

    #[test]
    fn panic_hook_bumps_panics_total_counter() {
        let handle = shared_handle();
        let before = counter_value(&handle.render(), PANICS_TOTAL);

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
