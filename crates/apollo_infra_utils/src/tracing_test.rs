use std::fmt::Debug;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::Duration;

use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::subscriber::{with_default, DefaultGuard};
use tracing::{Event, Level, Metadata, Subscriber};
use tracing_subscriber::fmt::SubscriberBuilder;

use crate::tracing::{CustomLogger, TraceLevel};
use crate::{
    debug_every_n,
    debug_every_n_sec,
    error_every_n,
    error_every_n_sec,
    info_every_n,
    info_every_n_sec,
    trace_every_n,
    trace_every_n_sec,
    warn_every_n,
    warn_every_n_sec,
};

#[test]
fn test_dynamic_logger_without_base_message() {
    let subscriber = TestSubscriber::new();

    with_default(subscriber.clone(), || {
        let logger = CustomLogger::new(TraceLevel::Info, None);
        logger.log_message("Test message");
    });

    let messages = subscriber.messages();
    assert_eq!(messages.len(), 1);
    assert!(messages[0].contains("Test message"));
    assert!(messages[0].contains("INFO"));
}

#[test]
fn test_dynamic_logger_with_base_message() {
    let subscriber = TestSubscriber::new();

    with_default(subscriber.clone(), || {
        let logger = CustomLogger::new(TraceLevel::Debug, Some("BaseMessage".to_string()));
        logger.log_message("Test message");
    });

    let messages = subscriber.messages();
    assert_eq!(messages.len(), 1);
    assert!(messages[0].contains("BaseMessage: Test message"));
    assert!(messages[0].contains("DEBUG"));
}

#[test]
fn test_all_trace_levels() {
    let subscriber = TestSubscriber::new();

    let test_cases = [
        (TraceLevel::Trace, "TRACE"),
        (TraceLevel::Debug, "DEBUG"),
        (TraceLevel::Info, "INFO"),
        (TraceLevel::Warn, "WARN"),
        (TraceLevel::Error, "ERROR"),
    ];

    with_default(subscriber.clone(), || {
        for (level, expected_level_str) in test_cases {
            subscriber.clear();
            let logger = CustomLogger::new(level, None);
            logger.log_message("Test message");

            let messages = subscriber.messages();
            assert_eq!(messages.len(), 1);
            assert!(messages[0].contains(expected_level_str));
            assert!(messages[0].contains("Test message"));
        }
    });
}

#[test]
fn test_message_formatting() {
    let subscriber = TestSubscriber::new();

    with_default(subscriber.clone(), || {
        let base_message = Some("Component".to_string());
        let logger = CustomLogger::new(TraceLevel::Info, base_message);
        logger.log_message("Operation completed");
    });

    let messages = subscriber.messages();
    assert_eq!(messages.len(), 1);
    assert!(messages[0].contains("Component: Operation completed"));
    assert!(messages[0].contains("INFO"));
}

#[test]
fn test_empty_message() {
    let subscriber = TestSubscriber::new();

    with_default(subscriber.clone(), || {
        let logger = CustomLogger::new(TraceLevel::Warn, None);
        logger.log_message("");
    });

    let messages = subscriber.messages();
    assert_eq!(messages.len(), 1);
    assert!(messages[0].contains("WARN"));
}

// Custom visitor to capture event fields
struct MessageVisitor<'a> {
    message: &'a mut String,
}

impl Visit for MessageVisitor<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        if field.name() == "message" {
            self.message.push_str(&format!("{:?}", value));
        }
    }
}

// Custom subscriber to capture log output
#[derive(Default, Clone)]
struct TestSubscriber {
    messages: Arc<Mutex<Vec<String>>>,
}

impl TestSubscriber {
    fn new() -> Self {
        Self { messages: Arc::new(Mutex::new(Vec::new())) }
    }

    fn messages(&self) -> Vec<String> {
        self.messages.lock().unwrap().clone()
    }

    fn clear(&self) {
        self.messages.lock().unwrap().clear();
    }
}

impl Subscriber for TestSubscriber {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn event(&self, event: &Event<'_>) {
        let mut message = String::new();
        let metadata = event.metadata();

        // Add level prefix
        message.push_str(&format!("{}: ", metadata.level()));

        // Capture the message field
        let mut visitor = MessageVisitor { message: &mut message };
        event.record(&mut visitor);

        self.messages.lock().unwrap().push(message);
    }

    fn enter(&self, _span: &Id) {}
    fn exit(&self, _span: &Id) {}

    fn new_span(&self, _span: &Attributes<'_>) -> Id {
        Id::from_u64(0)
    }

    fn record(&self, _span: &Id, _values: &Record<'_>) {}

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}
}

// Tests for the `log_every_n!` macros.

/// A struct used for capturing log outputs.
#[derive(Clone)]
struct SharedBuffer {
    inner: Arc<Mutex<Vec<u8>>>,
}

impl SharedBuffer {
    fn new() -> Self {
        SharedBuffer { inner: Arc::new(Mutex::new(Vec::new())) }
    }

    fn content(&self) -> String {
        let buffer = self.inner.lock().unwrap();
        String::from_utf8_lossy(&buffer).to_string()
    }
}

impl Write for SharedBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.inner.lock().unwrap().write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.lock().unwrap().flush()
    }
}

/// Sends logs to `SharedBuffer` (instead of stderr) for testing purposes.
/// Logs will be sent to the `buffer` until the returned `DefaultGuard` is dropped.
fn redirect_logs_to_buffer() -> (SharedBuffer, DefaultGuard) {
    let buffer = SharedBuffer::new();
    let buffer_clone = buffer.clone();

    let subscriber = SubscriberBuilder::default()
        .with_writer(move || buffer_clone.clone())
        .with_max_level(Level::TRACE)
        .with_ansi(false)
        .finish();

    let guard = tracing::subscriber::set_default(subscriber);

    assert!(buffer.content().is_empty(), "Buffer should be empty before logging");
    (buffer, guard)
}

const LOG_MESSAGE: &str = "Got logged";

// We test all the behaviors on one specific log level and then separately test that each macro logs
// at the correct level.

#[test]
fn test_log_every_n_logs_first_time() {
    let (buffer, _guard) = redirect_logs_to_buffer();

    warn_every_n!(1000, LOG_MESSAGE);

    assert_eq!(
        buffer.content().matches(LOG_MESSAGE).count(),
        1,
        "Log did not contain expected content:\n{}",
        buffer.content()
    );
}

#[test]
fn test_log_every_n_does_not_log_more_than_every_n() {
    let (buffer, _guard) = redirect_logs_to_buffer();

    for _ in 0..2 {
        warn_every_n!(3, LOG_MESSAGE);
    }

    assert_eq!(
        buffer.content().matches(LOG_MESSAGE).count(),
        1,
        "Log did not contain expected content:\n{}",
        buffer.content()
    );
}

#[test]
fn test_log_every_n_logs_every_n() {
    let (buffer, _guard) = redirect_logs_to_buffer();

    for _ in 0..5 {
        warn_every_n!(3, LOG_MESSAGE);
    }

    assert_eq!(
        buffer.content().matches(LOG_MESSAGE).count(),
        2,
        "Log did not contain expected content:\n{}",
        buffer.content()
    );
}

#[test]
fn test_log_every_n_different_lines_count_separately() {
    let (buffer, _guard) = redirect_logs_to_buffer();

    warn_every_n!(1000, LOG_MESSAGE);
    warn_every_n!(1000, LOG_MESSAGE);

    assert_eq!(
        buffer.content().matches(LOG_MESSAGE).count(),
        2,
        "Log did not contain expected content:\n{}",
        buffer.content()
    );
}

#[test]
fn test_trace_every_n_logs_to_trace() {
    let (buffer, _guard) = redirect_logs_to_buffer();

    trace_every_n!(2, LOG_MESSAGE);

    assert_eq!(
        buffer.content().matches("TRACE").count(),
        1,
        "Log did not contain expected TRACE content:\n{}",
        buffer.content()
    );
}

#[test]
fn test_debug_every_n_logs_to_debug() {
    let (buffer, _guard) = redirect_logs_to_buffer();

    debug_every_n!(2, LOG_MESSAGE);

    assert_eq!(
        buffer.content().matches("DEBUG").count(),
        1,
        "Log did not contain expected DEBUG content:\n{}",
        buffer.content()
    );
}

#[test]
fn test_info_every_n_logs_to_info() {
    let (buffer, _guard) = redirect_logs_to_buffer();

    info_every_n!(2, LOG_MESSAGE);

    assert_eq!(
        buffer.content().matches("INFO").count(),
        1,
        "Log did not contain expected INFO content:\n{}",
        buffer.content()
    );
}

#[test]
fn test_warn_every_n_logs_to_warn() {
    let (buffer, _guard) = redirect_logs_to_buffer();

    warn_every_n!(2, LOG_MESSAGE);

    assert_eq!(
        buffer.content().matches("WARN").count(),
        1,
        "Log did not contain expected WARN content:\n{}",
        buffer.content()
    );
}

#[test]
fn test_error_every_n_logs_to_error() {
    let (buffer, _guard) = redirect_logs_to_buffer();

    error_every_n!(2, LOG_MESSAGE);

    assert_eq!(
        buffer.content().matches("ERROR").count(),
        1,
        "Log did not contain expected ERROR content:\n{}",
        buffer.content()
    );
}

#[test]
fn test_log_every_n_sec_logs_first_time() {
    let (buffer, _guard) = redirect_logs_to_buffer();

    warn_every_n_sec!(1000, LOG_MESSAGE);

    assert_eq!(
        buffer.content().matches(LOG_MESSAGE).count(),
        1,
        "Log did not contain expected content:\n{}",
        buffer.content()
    );
}

// TODO(guy.f): Refactor the code so we can inject the time and don't need to use `sleep` in the
// tests below.

#[test]
fn test_log_every_n_sec_does_not_log_more_than_every_n() {
    let (buffer, _guard) = redirect_logs_to_buffer();

    for _ in 0..2 {
        warn_every_n_sec!(1, LOG_MESSAGE);
    }

    assert_eq!(
        buffer.content().matches(LOG_MESSAGE).count(),
        1,
        "Log did not contain expected content:\n{}",
        buffer.content()
    );
}

#[test]
fn test_log_every_n_logs_every_n_sec() {
    let (buffer, _guard) = redirect_logs_to_buffer();

    for _ in 0..5 {
        warn_every_n_sec!(2, LOG_MESSAGE);
        // Every second log should be logged due to the sleep.
        sleep(Duration::from_secs(1));
    }

    assert_eq!(
        buffer.content().matches(LOG_MESSAGE).count(),
        3,
        "Log did not contain expected content:\n{}",
        buffer.content()
    );
}

#[test]
fn test_log_every_n_sec_different_lines_count_separately() {
    let (buffer, _guard) = redirect_logs_to_buffer();

    warn_every_n_sec!(1, LOG_MESSAGE);
    sleep(Duration::from_secs(2));
    warn_every_n_sec!(1, LOG_MESSAGE);

    assert_eq!(
        buffer.content().matches(LOG_MESSAGE).count(),
        2,
        "Log did not contain expected content:\n{}",
        buffer.content()
    );
}

#[test]
fn test_trace_every_n_logs_to_trace_sec() {
    let (buffer, _guard) = redirect_logs_to_buffer();

    trace_every_n_sec!(2, LOG_MESSAGE);

    assert_eq!(
        buffer.content().matches("TRACE").count(),
        1,
        "Log did not contain expected TRACE content:\n{}",
        buffer.content()
    );
}

#[test]
fn test_debug_every_n_logs_to_debug_sec() {
    let (buffer, _guard) = redirect_logs_to_buffer();

    debug_every_n_sec!(2, LOG_MESSAGE);

    assert_eq!(
        buffer.content().matches("DEBUG").count(),
        1,
        "Log did not contain expected DEBUG content:\n{}",
        buffer.content()
    );
}

#[test]
fn test_info_every_n_logs_to_info_sec() {
    let (buffer, _guard) = redirect_logs_to_buffer();

    info_every_n_sec!(2, LOG_MESSAGE);

    assert_eq!(
        buffer.content().matches("INFO").count(),
        1,
        "Log did not contain expected INFO content:\n{}",
        buffer.content()
    );
}

#[test]
fn test_warn_every_n_logs_to_warn_sec() {
    let (buffer, _guard) = redirect_logs_to_buffer();

    warn_every_n_sec!(2, LOG_MESSAGE);

    assert_eq!(
        buffer.content().matches("WARN").count(),
        1,
        "Log did not contain expected WARN content:\n{}",
        buffer.content()
    );
}

#[test]
fn test_error_every_n_logs_to_error_sec() {
    let (buffer, _guard) = redirect_logs_to_buffer();

    error_every_n_sec!(2, LOG_MESSAGE);

    assert_eq!(
        buffer.content().matches("ERROR").count(),
        1,
        "Log did not contain expected ERROR content:\n{}",
        buffer.content()
    );
}
