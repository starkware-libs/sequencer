//! Propeller protocol configuration.

use std::time::Duration;

use libp2p::swarm::StreamProtocol;

/// The types of message validation that can be employed by Propeller.
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Clone, Debug)]
pub struct Config {
    /// Time to keep messages in cache.
    message_cache_ttl: Duration,
    /// Validation mode for incoming messages.
    validation_mode: ValidationMode,
    /// Stream protocol for the Propeller protocol.
    /// default is "/propeller/1.0.0"
    stream_protocol: StreamProtocol,
    /// Emit shard received events.
    emit_shard_received_events: bool,
    /// Maximum shard size in bytes
    max_shard_size: usize,
    /// Timeout for substream upgrades
    substream_timeout: Duration,
    /// If true the message will be padded to the nearest multiple of 2 * num_data_shards
    pad: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            message_cache_ttl: Duration::from_secs(20),
            validation_mode: ValidationMode::Strict,
            stream_protocol: StreamProtocol::new("/propeller/1.0.0"),
            emit_shard_received_events: false,
            max_shard_size: 1 << 20,
            substream_timeout: Duration::from_secs(30),
            pad: true,
        }
    }
}

impl Config {
    /// Time to keep messages in cache.
    pub fn message_cache_ttl(&self) -> Duration {
        self.message_cache_ttl
    }

    /// Get the validation mode for incoming messages.
    pub fn validation_mode(&self) -> &ValidationMode {
        &self.validation_mode
    }

    /// Get the stream protocol for the Propeller protocol.
    pub fn stream_protocol(&self) -> &StreamProtocol {
        &self.stream_protocol
    }

    /// Get the emit shard received events flag.
    pub fn emit_shard_received_events(&self) -> bool {
        self.emit_shard_received_events
    }

    /// Maximum shard size in bytes.
    pub fn max_shard_size(&self) -> usize {
        self.max_shard_size
    }

    /// Timeout for substream upgrades.
    pub fn substream_timeout(&self) -> Duration {
        self.substream_timeout
    }

    /// Get the pad flag.
    pub fn pad(&self) -> bool {
        self.pad
    }
}

/// Builder for Propeller configuration.
#[derive(Debug, Default)]
pub struct ConfigBuilder {
    config: Config,
}

impl ConfigBuilder {
    /// Set the message cache TTL.
    pub fn message_cache_ttl(mut self, ttl: Duration) -> Self {
        self.config.message_cache_ttl = ttl;
        self
    }

    /// Set the validation mode for incoming messages.
    pub fn validation_mode(mut self, validation_mode: ValidationMode) -> Self {
        self.config.validation_mode = validation_mode;
        self
    }

    /// Set the emit shard received events flag.
    pub fn emit_shard_received_events(mut self, emit_shard_received_events: bool) -> Self {
        self.config.emit_shard_received_events = emit_shard_received_events;
        self
    }

    /// Set the maximum shard size in bytes.
    pub fn max_shard_size(mut self, max_shard_size: usize) -> Self {
        self.config.max_shard_size = max_shard_size;
        self
    }

    /// Set the timeout for substream upgrades.
    pub fn substream_timeout(mut self, timeout: Duration) -> Self {
        self.config.substream_timeout = timeout;
        self
    }

    /// Set the pad flag.
    pub fn pad(mut self, pad: bool) -> Self {
        self.config.pad = pad;
        self
    }

    /// Build the configuration.
    pub fn build(self) -> Config {
        self.config
    }
}

impl Config {
    /// Create a new configuration builder.
    pub fn builder() -> ConfigBuilder {
        ConfigBuilder::default()
    }
}
