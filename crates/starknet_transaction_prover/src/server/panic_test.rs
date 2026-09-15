use std::panic::UnwindSafe;

use tracing_test::traced_test;

use crate::server::panic::{install_panic_hook, take_last_panic_location};

// Keep log assertions together so each payload has a single expected event.
#[test]
#[traced_test]
fn logs_structured_event_with_location_payload_and_backtrace() {
    // Recorded right next to the panic, so a hook that stops reading
    // `info.location()` (or hardcodes it) fails here instead of passing on a
    // file-only match.
    let static_panic_line = line!() + 1;
    catch_panic_under_hook(|| panic!("static literal"));
    let expected_location = format!("{}:{}:", file!(), static_panic_line);

    let panic_location = take_last_panic_location().expect("hook must record the panic's location");
    assert_eq!(panic_location.file, file!());
    assert_eq!(panic_location.line, static_panic_line);
    assert_eq!(
        take_last_panic_location(),
        None,
        "take_last_panic_location must clear the recorded location"
    );

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

    // `std::hint::black_box` blocks the compiler from constant-folding
    // `format_args!`. Without it, a placeholder with a literal argument can
    // collapse back into a `&'static str` (see the docs for
    // `Arguments::as_str`), and this would never exercise the
    // runtime-formatted `String` path.
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

    // `logs_contain` matches one substring at a time, so it cannot tell a
    // single well-formed event from fragments spread over several lines. Check
    // the whole line instead.
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

/// Leave the service hook installed for other tests that capture panic locations.
fn catch_panic_under_hook(panicking_body: impl FnOnce() + UnwindSafe) {
    install_panic_hook();
    let _ = std::panic::catch_unwind(panicking_body);
}
