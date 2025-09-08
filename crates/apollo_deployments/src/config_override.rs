use std::path::Path;

use apollo_infra_utils::dumping::serialize_to_file;
#[cfg(test)]
use apollo_infra_utils::dumping::serialize_to_file_test;
use apollo_network::serialize_multi_addrs;
use libp2p::Multiaddr;
use serde::{Serialize, Serializer};
use serde_json::to_value;
use serde_with::with_prefix;
use starknet_api::block::BlockNumber;
use url::Url;

use crate::deployment_definitions::{StateSyncConfig, StateSyncType};
#[cfg(test)]
use crate::test_utils::FIX_BINARY_NAME;

const DEPLOYMENT_FILE_NAME: &str = "deployment_config_override.json";

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct ConfigOverride {
    deployment_config_override: DeploymentConfigOverride,
    instance_config_override: InstanceConfigOverride,
}

impl ConfigOverride {
    pub const fn new(
        deployment_config_override: DeploymentConfigOverride,
        instance_config_override: InstanceConfigOverride,
    ) -> Self {
        Self { deployment_config_override, instance_config_override }
    }

    fn config_files(
        &self,
        deployment_config_override_dir: &Path,
        instance_name: &str,
        create: bool,
    ) -> ConfigOverrideWithPaths {
        let deployment_path = deployment_config_override_dir.join(DEPLOYMENT_FILE_NAME);
        let instance_path = deployment_config_override_dir.join(format!("{}.json", instance_name));

        if create {
            serialize_to_file(
                to_value(&self.deployment_config_override).unwrap(),
                deployment_path.to_str().unwrap(),
            );

            serialize_to_file(
                to_value(&self.instance_config_override).unwrap(),
                instance_path.to_str().unwrap(),
            );
        }

        ConfigOverrideWithPaths {
            #[cfg(test)]
            deployment_config_override: self.deployment_config_override.clone(),
            deployment_path: deployment_path.to_string_lossy().into_owned(),
            #[cfg(test)]
            instance_config_override: self.instance_config_override.clone(),
            instance_path: instance_path.to_string_lossy().into_owned(),
        }
    }

    pub fn get_config_file_paths(
        &self,
        deployment_config_override_dir: &Path,
        instance_name: &str,
    ) -> Vec<String> {
        let config_override_with_paths =
            self.config_files(deployment_config_override_dir, instance_name, false);
        vec![config_override_with_paths.deployment_path, config_override_with_paths.instance_path]
    }

    pub fn dump_config_files(
        &self,
        deployment_config_override_dir: &Path,
        instance_name: &str,
    ) -> Vec<String> {
        let config_override_with_paths =
            self.config_files(deployment_config_override_dir, instance_name, true);
        vec![config_override_with_paths.deployment_path, config_override_with_paths.instance_path]
    }

    #[cfg(test)]
    pub fn test_dump_config_files(
        &self,
        deployment_config_override_dir: &Path,
        instance_name: &str,
    ) {
        let config_override_with_paths =
            self.config_files(deployment_config_override_dir, instance_name, false);

        serialize_to_file_test(
            to_value(config_override_with_paths.deployment_config_override).unwrap(),
            &config_override_with_paths.deployment_path,
            FIX_BINARY_NAME,
        );

        serialize_to_file_test(
            to_value(config_override_with_paths.instance_config_override).unwrap(),
            &config_override_with_paths.instance_path,
            FIX_BINARY_NAME,
        );
    }
}

struct ConfigOverrideWithPaths {
    #[cfg(test)]
    deployment_config_override: DeploymentConfigOverride,
    deployment_path: String,
    #[cfg(test)]
    instance_config_override: InstanceConfigOverride,
    instance_path: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct DeploymentConfigOverride {
    #[serde(rename = "base_layer_config.starknet_contract_address")]
    starknet_contract_address: String,
    chain_id: String,
    eth_fee_token_address: String,
    starknet_url: Url,
    strk_fee_token_address: String,
    #[serde(rename = "l1_provider_config.provider_startup_height_override")]
    l1_provider_config_provider_startup_height_override: u64,
    #[serde(rename = "l1_provider_config.provider_startup_height_override.#is_none")]
    l1_provider_config_provider_startup_height_override_is_none: bool,
    #[serde(rename = "consensus_manager_config.context_config.num_validators")]
    consensus_manager_config_context_config_num_validators: usize,
    #[serde(flatten)]
    state_sync_config: StateSyncConfig,
}

impl DeploymentConfigOverride {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        starknet_contract_address: impl ToString,
        chain_id: impl ToString,
        eth_fee_token_address: impl ToString,
        starknet_url: Url,
        strk_fee_token_address: impl ToString,
        l1_startup_height_override: Option<BlockNumber>,
        consensus_manager_config_context_config_num_validators: usize,
        state_sync_type: StateSyncType,
    ) -> Self {
        let (
            l1_provider_config_provider_startup_height_override,
            l1_provider_config_provider_startup_height_override_is_none,
        ) = match l1_startup_height_override {
            Some(block_number) => (block_number.0, false),
            None => (0, true),
        };

        Self {
            starknet_contract_address: starknet_contract_address.to_string(),
            chain_id: chain_id.to_string(),
            eth_fee_token_address: eth_fee_token_address.to_string(),
            starknet_url,
            strk_fee_token_address: strk_fee_token_address.to_string(),
            l1_provider_config_provider_startup_height_override,
            l1_provider_config_provider_startup_height_override_is_none,
            consensus_manager_config_context_config_num_validators,
            state_sync_config: state_sync_type.get_state_sync_config(),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct NetworkConfigOverride {
    // Bootstrap peer address.
    #[serde(rename = "bootstrap_peer_multiaddr", serialize_with = "serialize_multi_addrs_wrapper")]
    bootstrap_peers_multiaddrs: Option<Vec<Multiaddr>>,
    #[serde(rename = "bootstrap_peer_multiaddr.#is_none")]
    bootstrap_peer_multiaddr_is_none: bool,

    // Advertised self address.
    #[serde(rename = "advertised_multiaddr")]
    advertised_multiaddr: Multiaddr,
    #[serde(rename = "advertised_multiaddr.#is_none")]
    advertised_multiaddr_is_none: bool,
}

impl NetworkConfigOverride {
    pub fn new(
        bootstrap_peers_multiaddrs: Option<Vec<Multiaddr>>,
        advertised_multiaddr: Option<Multiaddr>,
    ) -> Self {
        let (bootstrap_peers_multiaddrs, bootstrap_peer_multiaddr_is_none) =
            match bootstrap_peers_multiaddrs {
                Some(addrs) => (Some(addrs), false),
                None => (Some(vec![]), true),
            };
        let (advertised_multiaddr, advertised_multiaddr_is_none) = match advertised_multiaddr {
            Some(addr) => (addr, false),
            None => (Multiaddr::empty(), true),
        };
        Self {
            bootstrap_peers_multiaddrs,
            bootstrap_peer_multiaddr_is_none,
            advertised_multiaddr,
            advertised_multiaddr_is_none,
        }
    }
}

// Serialization prefixes for the network config overrides
with_prefix!(consensus_prefix "consensus_manager_config.network_config.");
with_prefix!(mempool_prefix "mempool_p2p_config.network_config.");

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct InstanceConfigOverride {
    #[serde(flatten, with = "consensus_prefix")]
    consensus_network_config_override: NetworkConfigOverride,
    #[serde(flatten, with = "mempool_prefix")]
    mempool_network_config_override: NetworkConfigOverride,
    validator_id: String,
}

impl InstanceConfigOverride {
    pub fn new(
        consensus_network_config_override: NetworkConfigOverride,
        mempool_network_config_override: NetworkConfigOverride,
        validator_id: impl ToString,
    ) -> Self {
        Self {
            consensus_network_config_override,
            mempool_network_config_override,
            validator_id: validator_id.to_string(),
        }
    }
}

// Wrapper function for the custom `serialize_multi_addrs` function, to be
// compatible with serde's `serialize_with` attribute. It first applies the custom serialization
// logic to convert the optional list into a `String`, and then serializes that string.
fn serialize_multi_addrs_wrapper<S>(
    optional_multi_addrs: &Option<Vec<Multiaddr>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match optional_multi_addrs {
        None => serializer.serialize_none(),
        Some(multi_addrs) => {
            // Call the implemented custom serialization function
            let s = serialize_multi_addrs(&Some(multi_addrs.clone()));
            // Serialize the returned String
            serializer.serialize_some(&s)
        }
    }
}
