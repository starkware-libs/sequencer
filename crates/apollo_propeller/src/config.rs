//! Propeller protocol configuration.

use std::time::Duration;

use libp2p::swarm::StreamProtocol;

/// The types of message validation that can be employed by Propeller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationMode {
    /// This is the default setting. This requires all messages to have valid signatures
    /// and the message author to be present and valid.
    Strict,
    /// This setting does not check the author or signature fields of incoming messages.
    /// If these fields contain data, they are simply ignored.
    ///
    /// NOTE: This setting will consider messages with invalid signatures as valid messages.
    None,
}

/// Configuration for the Propeller protocol.
#[derive(Clone)]
pub struct Config {
    /// Time to keep finalized messages in cache.
    pub finalized_message_ttl: Duration,
    /// Validation mode for incoming messages.
    pub validation_mode: ValidationMode,
    /// Stream protocol for the Propeller protocol.
    /// default is "/propeller/1.0.0"
    pub stream_protocol: StreamProtocol,
    /// Maximum size of a message sent over the wire
    pub max_wire_message_size: usize,
    /// If true the message will be padded to the nearest multiple of 2 * num_data_shards
    pub pad: bool,
    /// Timeout for validator and state manager tasks per message
    pub task_timeout: Duration,
    /// Capacity for bounded channels between tasks
    pub channel_capacity: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            finalized_message_ttl: Duration::from_secs(120),
            validation_mode: ValidationMode::Strict,
            stream_protocol: StreamProtocol::new("/propeller/1.0.0"),
            max_wire_message_size: 1 << 30,
            pad: true,
            task_timeout: Duration::from_secs(120),
            channel_capacity: 1 << 12,
        }
    }
}

impl Config {
    /// Create a new Config with default values.
    pub fn new() -> Self {
        Self::default()
    }
}
