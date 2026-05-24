use std::panic::UnwindSafe;
use std::sync::Mutex;

use tracing_test::traced_test;

use crate::server::metrics::names::PANICS_TOTAL;
use crate::server::panic::install_panic_hook;
use crate::server::test_recorder::{metric_value, shared_handle};

/// Serializes the tests that install the global panic hook and read the shared
/// `prover_panics_total` counter, so their before/after deltas don't interleave.
static PANIC_HOOK_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
#[traced_test]
fn logs_structured_event_with_location_payload_and_backtrace() {
    let _guard = PANIC_HOOK_TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    // Captured next to the panic so a hook that stops reading `info.location()`
    // (or hardcodes it) fails here instead of passing on a file-only match.
    let static_panic_line = line!() + 1;
    catch_panic_under_hook(|| panic!("static literal"));
    let expected_location = format!("{}:{}:", file!(), static_panic_line);

    assert!(logs_contain("Service panicked"), "must log a human-readable summary");
    assert!(logs_contain("event=\"panic\""), "must tag the event for log-based alerting");
    assert!(
        logs_contain(&format!("location={expected_location}")),
        "must record the panic's originating file and line"
    );
    assert!(
        logs_contain("payload=static literal"),
        "static-str payloads are reviewed source, safe to log verbatim"
    );
    assert!(
        logs_contain("backtrace=") && logs_contain("panic_hook"),
        "must capture a real backtrace, not an empty one"
    );

    // `std::hint::black_box` defeats the compiler's constant-folding of
    // `format_args!` (which can otherwise collapse a placeholder with a
    // literal argument back into a `&'static str` — see `Arguments::as_str`'s
    // docs) so this genuinely exercises the runtime-formatted `String` path.
    let runtime_value = std::hint::black_box(42);
    catch_panic_under_hook(|| panic!("formatted {} with request data", runtime_value));

    assert!(
        logs_contain("payload=<dynamic panic payload, redacted>"),
        "String payloads are built from runtime data and must be redacted, not echoed"
    );
    assert!(
        !logs_contain("formatted 42 with request data"),
        "the raw interpolated payload must never reach the log"
    );

    // `logs_contain` matches per-substring, so it cannot tell one well-formed
    // event from fragments spread across several lines. Pin the whole line.
    logs_assert(|lines: &[&str]| {
        let panic_lines: Vec<&&str> = lines
            .iter()
            .filter(|line| {
                line.contains("event=\"panic\"") && line.contains("payload=static literal")
            })
            .collect();
        let [line] = panic_lines[..] else {
            return Err(format!(
                "expected exactly one static-payload panic event, got {panic_lines:?}"
            ));
        };
        for field in [
            "ERROR",
            "event=\"panic\"",
            &format!("location={expected_location}"),
            "payload=static literal",
            "backtrace=",
            "Service panicked",
        ] {
            if !line.contains(field) {
                return Err(format!("panic event line is missing `{field}`: {line}"));
            }
        }
        Ok(())
    });
}

#[test]
fn panic_hook_bumps_panics_total_counter() {
    let _guard = PANIC_HOOK_TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let handle = shared_handle();
    let before = metric_value(&handle.render(), PANICS_TOTAL);

    catch_panic_under_hook(|| panic!("counter-test panic"));

    let after = metric_value(&handle.render(), PANICS_TOTAL);
    assert_eq!(after - before, 1.0);
}

/// Runs `panicking_body` with the service panic hook installed, swallowing the
/// unwind, and restores the hook that was in place beforehand.
fn catch_panic_under_hook(panicking_body: impl FnOnce() + UnwindSafe) {
    let previous_hook = std::panic::take_hook();
    install_panic_hook();
    let _ = std::panic::catch_unwind(panicking_body);
    std::panic::set_hook(previous_hook);
}
