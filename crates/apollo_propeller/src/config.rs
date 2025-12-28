//! Propeller protocol configuration.

use libp2p::swarm::StreamProtocol;

/// Configuration for the Propeller protocol.
#[derive(Clone)]
pub struct Config {
    /// Stream protocol for the Propeller protocol.
    stream_protocol: StreamProtocol,
    /// Maximum size of a message sent over the wire.
    max_wire_message_size: usize,
    /// Capacity for bounded channels between behaviour and core.
    channel_capacity: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            stream_protocol: StreamProtocol::new("/propeller/1.0.0"),
            max_wire_message_size: 1 << 30, // 1 GB
            channel_capacity: 1 << 12,      // 4096
        }
    }
}

impl Config {
    /// Create a new Config with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the stream protocol for the Propeller protocol.
    pub fn stream_protocol(&self) -> &StreamProtocol {
        &self.stream_protocol
    }

    /// Maximum size of a message sent over the wire.
    pub fn max_wire_message_size(&self) -> usize {
        self.max_wire_message_size
    }

    /// Get the channel capacity for inter-task communication.
    pub fn channel_capacity(&self) -> usize {
        self.channel_capacity
    }
}
