//! Propeller protocol configuration.

use std::time::Duration;

use libp2p::swarm::StreamProtocol;

/// Configuration for the Propeller protocol.
#[derive(Clone)]
pub struct Config {
    /// Timeout for stale messages (both cache TTL and task timeout).
    pub stale_message_timeout: Duration,
    /// Stream protocol for the Propeller protocol.
    pub stream_protocol: StreamProtocol,
    /// Maximum size of a message sent over the wire.
    pub max_wire_message_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            stale_message_timeout: Duration::from_secs(120),
            stream_protocol: StreamProtocol::new("/propeller/0.1.0"),
            max_wire_message_size: 1 << 20, // 1 MB
        }
    }
}
