/// This crate is responsible for sending messages to a given peer and responding to them according
/// to the [`Starknet p2p specs`]
///
/// [`Starknet p2p specs`]: https://github.com/starknet-io/starknet-p2p-specs/
pub mod authentication;
#[cfg(test)]
mod config_test;
mod discovery;
#[cfg(test)]
mod e2e_broadcast_test;
pub mod gossipsub_impl;
pub mod misconduct_score;
mod mixed_behaviour;
pub mod network_manager;
mod peer_manager;
mod sqmr;
#[cfg(test)]
mod test_utils;
pub mod utils;

use std::collections::{BTreeMap, HashSet};
use std::str::FromStr;
use std::time::Duration;

use apollo_config::converters::{
    deserialize_optional_vec_u8,
    deserialize_seconds_to_duration,
    serialize_optional_vec_u8,
};
use apollo_config::dumping::{
    prepend_sub_config_name,
    ser_optional_param,
    ser_param,
    SerializeConfig,
};
use apollo_config::validators::validate_vec_u256;
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use discovery::DiscoveryConfig;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::Multiaddr;
use peer_manager::PeerManagerConfig;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};
use starknet_api::core::ChainId;
use validator::{Validate, ValidationError};

// TODO(AndrewL): Fix this
/// This function considers `""` to be `None` and
/// `"multiaddr1,multiaddr2"` to be `Some(vec![multiaddr1, multiaddr2])`.
/// It was purposefully designed this way to be compatible with the old config where only one
/// bootstrap peer was supported. Hence there is no way to express an empty vector in the config.
fn deserialize_multi_addrs<'de, D>(de: D) -> Result<Option<Vec<Multiaddr>>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw_str: String = Deserialize::deserialize(de).unwrap_or_default();
    if raw_str.is_empty() {
        return Ok(None);
    }

    let mut vector = Vec::new();
    for i in raw_str.split(',').filter(|s| !s.is_empty()) {
        let value = Multiaddr::from_str(i).map_err(|_| {
            D::Error::custom(format!("Couldn't deserialize vector. Failed to parse value: {i}"))
        })?;
        vector.push(value);
    }

    if vector.is_empty() {
        return Ok(None);
    }

    Ok(Some(vector))
}

fn serialize_multi_addrs(multi_addrs: &Option<Vec<Multiaddr>>) -> String {
    match multi_addrs {
        None => "".to_owned(),
        Some(multi_addrs) => multi_addrs
            .iter()
            .map(|multiaddr| multiaddr.to_string())
            .collect::<Vec<String>>()
            .join(","),
    }
}

// TODO(Shahak): add peer manager config to the network config
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Validate)]
pub struct NetworkConfig {
    pub port: u16,
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub session_timeout: Duration,
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub idle_connection_timeout: Duration,
    #[serde(deserialize_with = "deserialize_multi_addrs")]
    #[validate(custom(function = "validate_bootstrap_peer_multiaddr_list"))]
    pub bootstrap_peer_multiaddr: Option<Vec<Multiaddr>>,
    #[validate(custom = "validate_vec_u256")]
    #[serde(deserialize_with = "deserialize_optional_vec_u8")]
    pub secret_key: Option<Vec<u8>>,
    pub advertised_multiaddr: Option<Multiaddr>,
    pub chain_id: ChainId,
    pub discovery_config: DiscoveryConfig,
    pub peer_manager_config: PeerManagerConfig,
    pub broadcasted_message_metadata_buffer_size: usize,
    pub reported_peer_ids_buffer_size: usize,
}

impl SerializeConfig for NetworkConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::from_iter([
            ser_param(
                "port",
                &self.port,
                "The port that the node listens on for incoming udp connections for quic.",
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
            ser_param(
                "broadcasted_message_metadata_buffer_size",
                &self.broadcasted_message_metadata_buffer_size,
                "The size of the buffer that holds the metadata of the broadcasted messages.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "reported_peer_ids_buffer_size",
                &self.reported_peer_ids_buffer_size,
                "The size of the buffer that holds the reported peer ids.",
                ParamPrivacyInput::Public,
            ),
        ]);
        config.extend(ser_optional_param(
            &if self.bootstrap_peer_multiaddr.is_some(){
                Some(serialize_multi_addrs(&self.bootstrap_peer_multiaddr))
            } else {
                None
            },
            String::from(""),
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
            &self.advertised_multiaddr,
            Multiaddr::empty(),
            "advertised_multiaddr",
            "The external address other peers see this node. If this is set, the node will not \
             try to find out which addresses it has and will write this address as external \
             instead",
            ParamPrivacyInput::Public,
        ));
        config.extend(prepend_sub_config_name(self.discovery_config.dump(), "discovery_config"));
        config.extend(prepend_sub_config_name(
            self.peer_manager_config.dump(),
            "peer_manager_config",
        ));
        config
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            port: 10000,
            session_timeout: Duration::from_secs(120),
            idle_connection_timeout: Duration::from_secs(120),
            bootstrap_peer_multiaddr: None,
            secret_key: None,
            advertised_multiaddr: None,
            chain_id: ChainId::Mainnet,
            discovery_config: DiscoveryConfig::default(),
            peer_manager_config: PeerManagerConfig::default(),
            broadcasted_message_metadata_buffer_size: 100000,
            reported_peer_ids_buffer_size: 100000,
        }
    }
}

/// Validates a list of bootstrap peers.
///
/// The list must be comprised of `Multiaddr` each containing a `PeerId`.
/// Each `PeerId` must be unique in the list.
fn validate_bootstrap_peer_multiaddr_list(
    bootstrap_peer_multiaddr: &[Multiaddr],
) -> Result<(), validator::ValidationError> {
    let mut peers = HashSet::new();
    for address in bootstrap_peer_multiaddr.iter() {
        let Some(peer_id) = DialOpts::from(address.clone()).get_peer_id() else {
            return Err(ValidationError::new(
                "Bootstrap peer Multiaddr does not contain a PeerId.",
            ));
        };

        if !peers.insert(peer_id) {
            let mut error = ValidationError::new("Bootstrap peer PeerIds are not unique.");
            error.message = Some(std::borrow::Cow::from(format!("Repeated PeerId: {peer_id}")));
            return Err(error);
        }
    }
    Ok(())
}
