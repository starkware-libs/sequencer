use tracing::{debug, error, info, trace, warn};

/// Dynamically set tracing level of a message.
pub struct DynamicLogger {
    level: TraceLevel,
    base_message: Option<String>,
}

impl DynamicLogger {
    /// Creates a new trace configuration
    pub fn new(level: TraceLevel, base_message: Option<String>) -> Self {
        Self { level, base_message }
    }

    /// Logs a given message at the specified tracing level, concatenated with the base message if
    /// it exists.
    pub fn log_message(&self, message: &str) {
        let message = match &self.base_message {
            Some(base_message) => format!("{}: {}", base_message, message),
            None => message.to_string(),
        };

        match self.level {
            TraceLevel::Trace => trace!(message),
            TraceLevel::Debug => debug!(message),
            TraceLevel::Info => info!(message),
            TraceLevel::Warn => warn!(message),
            TraceLevel::Error => error!(message),
        }
    }
}

#[derive(Clone, Copy)]
pub enum TraceLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}
