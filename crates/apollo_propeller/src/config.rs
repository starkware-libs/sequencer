//! Propeller protocol configuration.

use libp2p::swarm::StreamProtocol;

/// Configuration for the Propeller protocol.
#[derive(Clone)]
pub struct Config {
    /// Stream protocol for the Propeller protocol.
    pub stream_protocol: StreamProtocol,
    /// Maximum size of a message sent over the wire.
    pub max_wire_message_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            stream_protocol: StreamProtocol::new("/propeller/0.1.0"),
            max_wire_message_size: 1 << 20, // 1 MB
        }
    }
}
