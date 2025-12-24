pub use apollo_proc_macros::{log_every_n, log_every_n_ms};
use tracing::{debug, error, info, trace, warn};

#[cfg(test)]
#[path = "tracing_test.rs"]
mod tracing_test;

/// Enable setting a message tracing level at runtime.
pub struct CustomLogger {
    level: TraceLevel,
    base_message: Option<String>,
}

impl CustomLogger {
    /// Creates a new trace configuration
    pub fn new(level: TraceLevel, base_message: Option<String>) -> Self {
        Self { level, base_message }
    }

    /// Logs a given message at the specified tracing level, concatenated with the base message if
    /// it exists.
    pub fn log_message(&self, message: &str) {
        let message = match &self.base_message {
            Some(base_message) => format!("{base_message}: {message}"),
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

pub trait LogCompatibleToStringExt: std::fmt::Display {
    fn log_compatible_to_string(&self) -> String {
        self.to_string().replace('\n', "\t")
    }
}

/// Logs an INFO message once every `n` calls.
///
/// Each call site of this macro maintains its own independent counter.
/// The message will be logged on calls: 1, N+1, 2N+1, 3N+1, etc., for each invocation **from that
/// specific call site**.
///
/// # Arguments
///
/// * `$n`: The integer frequency (e.g., `2` for every second call).
/// * `$($arg:tt)*`: The arguments to pass to `tracing::info!`, e.g., a format string and its
///   corresponding values.
///
/// # Example
/// ```rust
/// use apollo_infra_utils::info_every_n;
///
/// for i in 0..5 {
///     info_every_n!(2, "Processing item: {}", i);
///     // Output:
///     // Processing item: 0 (on 1st call)
///     // Processing item: 2 (on 3rd call)
///     // Processing item: 4 (on 5th call)
/// }
///
/// // This will log twice since these are two different call sites.
/// info_every_n!(2, "call site");
/// info_every_n!(2, "call site");
/// ```
#[macro_export]
macro_rules! info_every_n {
    ($n:expr, $($arg:tt)*) => {
        {
            $crate::_apollo_proc_macros::log_every_n!(::tracing::info, $n, $($arg)*);
        }
    };
}

/// Logs a WARN message once every `n` calls.
/// See `info_every_n!` for detailed usage and behavior.
#[macro_export]
macro_rules! warn_every_n {
    ($n:expr, $($arg:tt)*) => {
        {
            $crate::_apollo_proc_macros::log_every_n!(::tracing::warn, $n, $($arg)*);
        }
    };
}

/// Logs an ERROR message once every `n` calls.
/// See `info_every_n!` for detailed usage and behavior.
#[macro_export]
macro_rules! error_every_n {
    ($n:expr, $($arg:tt)*) => {
        {
            $crate::_apollo_proc_macros::log_every_n!(::tracing::error, $n, $($arg)*);
        }
    };
}

/// Logs a DEBUG message once every `n` calls.
/// See `info_every_n!` for detailed usage and behavior.
#[macro_export]
macro_rules! debug_every_n {
    ($n:expr, $($arg:tt)*) => {
        {
            $crate::_apollo_proc_macros::log_every_n!(::tracing::debug, $n, $($arg)*);
        }
    };
}

/// Logs a TRACE message once every `n` calls.
/// See `info_every_n!` for detailed usage and behavior.
#[macro_export]
macro_rules! trace_every_n {
    ($n:expr, $($arg:tt)*) => {
        {
            $crate::_apollo_proc_macros::log_every_n!(::tracing::trace, $n, $($arg)*);
        }
    };
}

/// Logs an INFO message once every `n` milliseconds.
///
/// Each call site of this macro maintains its own independent timer.
///
/// # Arguments
///
/// * `$n`: Number of milliseconds (e.g., `2` for not logging a message again for `2`` milliseconds
///   from when it last logged).
/// * `$($arg:tt)*`: The arguments to pass to `tracing::info!`, e.g., a format string and its
///   corresponding values.
///
/// # Example
/// ```rust
/// use apollo_infra_utils::info_every_n_ms;
///
/// fn do_something(i: u32) {
///     // Work...
///     info_every_n_ms!(5, "Processing item: {}", i);
///     // Work...
/// }
///
/// do_something(0);
/// // 2 seconds pass.
/// do_something(1); // No log, only 2 seconds passed.
/// // 4 seconds pass.
/// do_something(2); // Logs: "Processing item: 2" (6 seconds passed since last log).
/// // 4 seconds pass.
/// do_something(3); // No log, only 4 seconds passed since **last** log.
///
/// // This will log twice since these are two different call sites.
/// info_every_n_ms!(2, "call site");
/// info_every_n_ms!(2, "call site");
/// ```
#[macro_export]
macro_rules! info_every_n_ms {
    ($n:expr, $($arg:tt)*) => {
        {
            $crate::_apollo_proc_macros::log_every_n_ms!(::tracing::info, $n, $($arg)*);
        }
    };
}

/// Logs a WARN message once every `n` seconds.
/// See `info_every_n_ms!` for detailed usage and behavior.
#[macro_export]
macro_rules! warn_every_n_ms {
    ($n:expr, $($arg:tt)*) => {
        {
            $crate::_apollo_proc_macros::log_every_n_ms!(::tracing::warn, $n, $($arg)*);
        }
    };
}

/// Logs an ERROR message once every `n` seconds.
/// See `info_every_n_ms!` for detailed usage and behavior.
#[macro_export]
macro_rules! error_every_n_ms {
    ($n:expr, $($arg:tt)*) => {
        {
            $crate::_apollo_proc_macros::log_every_n_ms!(::tracing::error, $n, $($arg)*);
        }
    };
}

/// Logs a DEBUG message once every `n` seconds.
/// See `info_every_n_ms!` for detailed usage and behavior.
#[macro_export]
macro_rules! debug_every_n_ms {
    ($n:expr, $($arg:tt)*) => {
        {
            $crate::_apollo_proc_macros::log_every_n_ms!(::tracing::debug, $n, $($arg)*);
        }
    };
}

/// Logs a TRACE message once every `n` seconds.
/// See `info_every_n_ms!` for detailed usage and behavior.
#[macro_export]
macro_rules! trace_every_n_ms {
    ($n:expr, $($arg:tt)*) => {
        {
            $crate::_apollo_proc_macros::log_every_n_ms!(::tracing::trace, $n, $($arg)*);
        }
    };
}
