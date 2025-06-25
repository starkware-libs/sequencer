use std::path::Path;

use apollo_infra_utils::dumping::serialize_to_file;
#[cfg(test)]
use apollo_infra_utils::dumping::serialize_to_file_test;
use serde::Serialize;
use serde_json::to_value;
use serde_with::with_prefix;
use starknet_api::block::BlockNumber;

use crate::deployment::PragmaDomain;
#[cfg(test)]
use crate::test_utils::FIX_BINARY_NAME;

const DEPLOYMENT_FILE_NAME: &str = "deployment_config_override.json";
const INSTANCE_FILE_NAME: &str = "instance_config_override.json";

const PRAGMA_URL_TEMPLATE: &str =
    "https://api.{}.pragma.build/node/v1/data/eth/strk?interval=15min&aggregation=median";

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
        application_config_subdir: &Path,
        create: bool,
    ) -> ConfigOverrideWithPaths {
        let deployment_path = application_config_subdir.join(DEPLOYMENT_FILE_NAME);
        let instance_path = application_config_subdir.join(INSTANCE_FILE_NAME);

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

    pub fn get_config_file_paths(&self, application_config_subdir: &Path) -> Vec<String> {
        let config_override_with_paths = self.config_files(application_config_subdir, false);
        vec![config_override_with_paths.deployment_path, config_override_with_paths.instance_path]
    }

    pub fn dump_config_files(&self, application_config_subdir: &Path) -> Vec<String> {
        let config_override_with_paths = self.config_files(application_config_subdir, true);
        vec![config_override_with_paths.deployment_path, config_override_with_paths.instance_path]
    }

    #[cfg(test)]
    pub fn test_dump_config_files(&self, application_config_subdir: &Path) {
        let config_override_with_paths = self.config_files(application_config_subdir, false);

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
    starknet_url: String,
    strk_fee_token_address: String,
    #[serde(rename = "consensus_manager_config.eth_to_strk_oracle_config.base_url")]
    consensus_manager_config_eth_to_strk_oracle_config_base_url: String,
    #[serde(rename = "l1_provider_config.provider_startup_height_override")]
    l1_provider_config_provider_startup_height_override: u64,
    #[serde(rename = "l1_provider_config.provider_startup_height_override.#is_none")]
    l1_provider_config_provider_startup_height_override_is_none: bool,
}

impl DeploymentConfigOverride {
    pub fn new(
        starknet_contract_address: impl ToString,
        chain_id: impl ToString,
        eth_fee_token_address: impl ToString,
        starknet_url: impl ToString,
        strk_fee_token_address: impl ToString,
        pragma_domain: PragmaDomain,
        l1_startup_height_override: Option<BlockNumber>,
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
            starknet_url: starknet_url.to_string(),
            strk_fee_token_address: strk_fee_token_address.to_string(),
            consensus_manager_config_eth_to_strk_oracle_config_base_url: PRAGMA_URL_TEMPLATE
                .replace("{}", &pragma_domain.to_string()),
            l1_provider_config_provider_startup_height_override,
            l1_provider_config_provider_startup_height_override_is_none,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct NetworkConfigOverride {
    // Bootstrap peer address.
    #[serde(rename = "bootstrap_peer_multiaddr")]
    bootstrap_peer_multiaddr: String,
    #[serde(rename = "bootstrap_peer_multiaddr.#is_none")]
    bootstrap_peer_multiaddr_is_none: bool,

    // Advertised self address.
    #[serde(rename = "advertised_multiaddr")]
    advertised_multiaddr: String,
    #[serde(rename = "advertised_multiaddr.#is_none")]
    advertised_multiaddr_is_none: bool,

    // TODO(Tsabary): network secret keys should be defined as secrets.
    secret_key: String,
}

impl NetworkConfigOverride {
    pub fn new(
        bootstrap_peer_multiaddr: Option<String>,
        advertised_multiaddr: Option<String>,
        secret_key: impl ToString,
    ) -> Self {
        let (bootstrap_peer_multiaddr, bootstrap_peer_multiaddr_is_none) =
            match bootstrap_peer_multiaddr {
                Some(addr) => (addr, false),
                None => ("".to_string(), true),
            };
        let (advertised_multiaddr, advertised_multiaddr_is_none) = match advertised_multiaddr {
            Some(addr) => (addr, false),
            None => ("".to_string(), true),
        };
        Self {
            bootstrap_peer_multiaddr,
            bootstrap_peer_multiaddr_is_none,
            advertised_multiaddr,
            advertised_multiaddr_is_none,
            secret_key: secret_key.to_string(),
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
