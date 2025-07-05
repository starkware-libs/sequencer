use std::fmt::Debug;
use std::sync::{Arc, Mutex};

use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::subscriber::with_default;
use tracing::{Event, Metadata, Subscriber};

use crate::tracing::{CustomLogger, TraceLevel};

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
            self.message.push_str(&format!("{value:?}"));
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
