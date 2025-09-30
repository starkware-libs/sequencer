//! Propeller protocol configuration.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use libp2p::swarm::StreamProtocol;
use libp2p::PeerId;

use crate::PropellerUnit;

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

pub type MaliceFunction =
    Arc<Mutex<dyn FnMut(PeerId, PropellerUnit) -> Option<PropellerUnit> + Send>>;

/// Configuration for the Propeller protocol.
#[derive(Clone)]
pub struct Config {
    /// Time to keep finalized messages in cache.
    finalized_message_ttl: Duration,
    /// Validation mode for incoming messages.
    validation_mode: ValidationMode,
    /// Stream protocol for the Propeller protocol.
    /// default is "/propeller/1.0.0"
    stream_protocol: StreamProtocol,
    /// Maximum size of a message sent over the wire
    max_wire_message_size: usize,
    /// Timeout for substream upgrades
    substream_timeout: Duration,
    /// If true the message will be padded to the nearest multiple of 2 * num_data_shards
    pad: bool,
    /// Function to modify the message before it is broadcasted. Used for testing purposes.
    malice_function: Option<MaliceFunction>,
    /// Timeout for validator and state manager tasks per message
    task_timeout: Duration,
    /// Capacity for bounded channels between tasks
    channel_capacity: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            finalized_message_ttl: Duration::from_secs(120),
            validation_mode: ValidationMode::Strict,
            stream_protocol: StreamProtocol::new("/propeller/1.0.0"),
            max_wire_message_size: 1 << 30,
            substream_timeout: Duration::from_secs(30),
            pad: true,
            malice_function: None,
            task_timeout: Duration::from_secs(120),
            channel_capacity: 1 << 12,
        }
    }
}

impl Config {
    /// Time to keep messages in cache.
    pub fn finalized_message_ttl(&self) -> Duration {
        self.finalized_message_ttl
    }

    /// Get the validation mode for incoming messages.
    pub fn validation_mode(&self) -> &ValidationMode {
        &self.validation_mode
    }

    /// Get the stream protocol for the Propeller protocol.
    pub fn stream_protocol(&self) -> &StreamProtocol {
        &self.stream_protocol
    }

    /// Maximum size of a message sent over the wire.
    pub fn max_wire_message_size(&self) -> usize {
        self.max_wire_message_size
    }

    /// Timeout for substream upgrades.
    pub fn substream_timeout(&self) -> Duration {
        self.substream_timeout
    }

    /// Get the pad flag.
    pub fn pad(&self) -> bool {
        self.pad
    }

    /// Get the task timeout duration.
    pub fn task_timeout(&self) -> Duration {
        self.task_timeout
    }

    /// Get the channel capacity for inter-task communication.
    pub fn channel_capacity(&self) -> usize {
        self.channel_capacity
    }

    pub fn malice_modify(&mut self, peer: PeerId, unit: PropellerUnit) -> Option<PropellerUnit> {
        let Some(malice_function) = self.malice_function.as_mut() else { return Some(unit) };
        malice_function.lock().unwrap()(peer, unit)
    }
}

/// Builder for Propeller configuration.
#[derive(Default)]
pub struct ConfigBuilder {
    config: Config,
}

impl ConfigBuilder {
    /// Set the message cache TTL.
    pub fn finalized_message_ttl(mut self, ttl: Duration) -> Self {
        self.config.finalized_message_ttl = ttl;
        self
    }

    /// Set the validation mode for incoming messages.
    pub fn validation_mode(mut self, validation_mode: ValidationMode) -> Self {
        self.config.validation_mode = validation_mode;
        self
    }

    /// Set the maximum size of a message sent over the wire.
    pub fn max_wire_message_size(mut self, max_wire_message_size: usize) -> Self {
        self.config.max_wire_message_size = max_wire_message_size;
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

    /// Set the malice function.
    pub fn malice_function(mut self, malice_function: MaliceFunction) -> Self {
        self.config.malice_function = Some(malice_function);
        self
    }

    /// Set the task timeout duration.
    pub fn task_timeout(mut self, timeout: Duration) -> Self {
        self.config.task_timeout = timeout;
        self
    }

    /// Set the channel capacity for inter-task communication.
    pub fn channel_capacity(mut self, capacity: usize) -> Self {
        self.config.channel_capacity = capacity;
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
