use tracing_test::traced_test;

use crate::server::panic::install_panic_hook;

// The panic hook is global state — a single #[test] keeps captures serial.
#[test]
#[traced_test]
fn logs_structured_event_with_location_payload_and_backtrace() {
    let prev_hook = std::panic::take_hook();
    install_panic_hook();
    let _ = std::panic::catch_unwind(|| panic!("static literal"));
    // `std::hint::black_box` defeats the compiler's constant-folding of
    // `format_args!` (which can otherwise collapse a placeholder with a
    // literal argument back into a `&'static str` — see `Arguments::as_str`'s
    // docs) so this genuinely exercises the runtime-formatted `String` path.
    let runtime_value = std::hint::black_box(42);
    let _ = std::panic::catch_unwind(|| panic!("formatted {} with request data", runtime_value));
    std::panic::set_hook(prev_hook);

    assert!(logs_contain("Service panicked"), "must log a human-readable summary");
    assert!(logs_contain("event=\"panic\""), "must tag the event for log-based alerting");
    assert!(
        logs_contain(&format!("location={}:", file!())),
        "must record the panic's originating file"
    );
    assert!(
        logs_contain("payload=static literal"),
        "static-str payloads are reviewed source, safe to log verbatim"
    );
    assert!(
        logs_contain("payload=<dynamic panic payload, redacted>"),
        "String payloads are built from runtime data and must be redacted, not echoed"
    );
    assert!(
        !logs_contain("formatted 42 with request data"),
        "the raw interpolated payload must never reach the log"
    );
    assert!(
        logs_contain("backtrace=") && logs_contain("panic_hook"),
        "must capture a real backtrace, not an empty one"
    );
}
