/// This crate is responsible for sending messages to a given peer and responding to them according
/// to the [`Starknet p2p specs`]
///
/// [`Starknet p2p specs`]: https://github.com/starknet-io/starknet-p2p-specs/
mod bin_utils;
mod discovery;
#[cfg(test)]
mod e2e_broadcast_test;
pub mod gossipsub_impl;
mod mixed_behaviour;
pub mod network_manager;
mod peer_manager;
mod sqmr;
#[cfg(test)]
mod test_utils;
pub mod utils;

use std::collections::BTreeMap;
use std::time::Duration;

use discovery::DiscoveryConfig;
use libp2p::Multiaddr;
use papyrus_config::converters::{
    deserialize_optional_vec_u8,
    deserialize_seconds_to_duration,
    serialize_optional_vec_u8,
};
use papyrus_config::dumping::{
    append_sub_config_name,
    ser_optional_param,
    ser_param,
    SerializeConfig,
};
use papyrus_config::validators::validate_vec_u256;
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use peer_manager::PeerManagerConfig;
use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;
use validator::Validate;

// TODO: add peer manager config to the network config
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Validate)]
pub struct NetworkConfig {
    pub tcp_port: u16,
    pub quic_port: u16,
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub session_timeout: Duration,
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub idle_connection_timeout: Duration,
    pub bootstrap_peer_multiaddr: Option<Multiaddr>,
    #[validate(custom = "validate_vec_u256")]
    #[serde(deserialize_with = "deserialize_optional_vec_u8")]
    pub(crate) secret_key: Option<Vec<u8>>,
    pub advertised_multiaddr: Option<Multiaddr>,
    pub chain_id: ChainId,
    pub discovery_config: DiscoveryConfig,
    pub peer_manager_config: PeerManagerConfig,
}

impl SerializeConfig for NetworkConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::from_iter([
            ser_param(
                "tcp_port",
                &self.tcp_port,
                "The port that the node listens on for incoming tcp connections.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "quic_port",
                &self.quic_port,
                "The port that the node listens on for incoming quic connections.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "session_timeout",
                &self.session_timeout.as_secs(),
                "Maximal time in seconds that each session can take before failing on timeout.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "idle_connection_timeout",
                &self.idle_connection_timeout.as_secs(),
                "Amount of time in seconds that a connection with no active sessions will stay \
                 alive.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "chain_id",
                &self.chain_id,
                "The chain to follow. For more details see https://docs.starknet.io/documentation/architecture_and_concepts/Blocks/transactions/#chain-id.",
                ParamPrivacyInput::Public,
            ),
        ]);
        config.extend(ser_optional_param(
            &self.bootstrap_peer_multiaddr,
            Multiaddr::empty(),
            "bootstrap_peer_multiaddr",
            "The multiaddress of the peer node. It should include the peer's id. For more info: https://docs.libp2p.io/concepts/fundamentals/peers/",
            ParamPrivacyInput::Public,
        ));
        config.extend([ser_param(
            "secret_key",
            &serialize_optional_vec_u8(&self.secret_key),
            "The secret key used for building the peer id. If it's an empty string a random one \
             will be used.",
            ParamPrivacyInput::Private,
        )]);
        config.extend(ser_optional_param(
            &self.bootstrap_peer_multiaddr,
            Multiaddr::empty(),
            "advertised_multiaddr",
            "The external address other peers see this node. If this is set, the node will not \
             try to find out which addresses it has and will write this address as external \
             instead",
            ParamPrivacyInput::Public,
        ));
        config.extend(append_sub_config_name(self.discovery_config.dump(), "discovery_config"));
        config
            .extend(append_sub_config_name(self.peer_manager_config.dump(), "peer_manager_config"));
        config
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            tcp_port: 10000,
            quic_port: 10001,
            session_timeout: Duration::from_secs(120),
            idle_connection_timeout: Duration::from_secs(120),
            bootstrap_peer_multiaddr: None,
            secret_key: None,
            advertised_multiaddr: None,
            chain_id: ChainId::Mainnet,
            discovery_config: DiscoveryConfig::default(),
            peer_manager_config: PeerManagerConfig::default(),
        }
    }
}
