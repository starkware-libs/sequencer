//! Propeller protocol configuration.

use std::time::Duration;

use libp2p::swarm::StreamProtocol;

/// Configuration for the Propeller protocol.
#[derive(Clone, Debug)]
pub struct Config {
    /// Timeout for stale messages (both cache TTL and task timeout).
    pub stale_message_timeout: Duration,
    /// Stream protocol for the Propeller protocol.
    // TODO(AndrewL): In the apollo config, this field should be a constant and not part of the
    // config.
    pub stream_protocol: StreamProtocol,
    /// Maximum size of a message sent over the wire.
    pub max_wire_message_size: usize,
    /// Capacity of the bounded channel between each handler and the engine for inbound units.
    /// Controls back-pressure: when the channel is full, the handler stops reading from the
    /// network, causing yamux flow control to slow the remote peer.
    pub inbound_channel_capacity: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            stale_message_timeout: Duration::from_secs(120),
            stream_protocol: StreamProtocol::new("/propeller/0.1.0"),
            max_wire_message_size: 1 << 20, // 1 MB
            inbound_channel_capacity: 256,
        }
    }
}
