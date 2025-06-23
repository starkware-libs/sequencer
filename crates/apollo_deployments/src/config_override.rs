use std::path::Path;

use apollo_infra_utils::dumping::serialize_to_file;
#[cfg(test)]
use apollo_infra_utils::dumping::serialize_to_file_test;
use serde::Serialize;
use serde_json::to_value;
use starknet_api::block::BlockNumber;

use crate::deployment::PragmaDomain;
#[cfg(test)]
use crate::deployment::FIX_BINARY_NAME;

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
            // TODO(Tsabary): use `format!` instead.
            consensus_manager_config_eth_to_strk_oracle_config_base_url: PRAGMA_URL_TEMPLATE
                .replace("{}", &pragma_domain.to_string()),
            l1_provider_config_provider_startup_height_override,
            l1_provider_config_provider_startup_height_override_is_none,
        }
    }
}

// TODO(Tsabary): re-verify all config diffs.

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct InstanceConfigOverride {
    #[serde(rename = "consensus_manager_config.network_config.bootstrap_peer_multiaddr")]
    consensus_bootstrap_peer_multiaddr: String,
    #[serde(rename = "consensus_manager_config.network_config.bootstrap_peer_multiaddr.#is_none")]
    consensus_bootstrap_peer_multiaddr_is_none: bool,
    // TODO(Tsabary): network secret keys should be defined as secrets.
    #[serde(rename = "consensus_manager_config.network_config.secret_key")]
    consensus_secret_key: String,
    #[serde(rename = "mempool_p2p_config.network_config.bootstrap_peer_multiaddr")]
    mempool_bootstrap_peer_multiaddr: String,
    #[serde(rename = "mempool_p2p_config.network_config.bootstrap_peer_multiaddr.#is_none")]
    mempool_bootstrap_peer_multiaddr_is_none: bool,
    // TODO(Tsabary): network secret keys should be defined as secrets.
    #[serde(rename = "mempool_p2p_config.network_config.secret_key")]
    mempool_secret_key: String,
    validator_id: String,
}

impl InstanceConfigOverride {
    // TODO(Tsabary): reduce number of args.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        consensus_bootstrap_peer_multiaddr: impl ToString,
        consensus_bootstrap_peer_multiaddr_is_none: bool,
        consensus_secret_key: impl ToString,
        mempool_bootstrap_peer_multiaddr: impl ToString,
        mempool_bootstrap_peer_multiaddr_is_none: bool,
        mempool_secret_key: impl ToString,
        validator_id: impl ToString,
    ) -> Self {
        Self {
            consensus_bootstrap_peer_multiaddr: consensus_bootstrap_peer_multiaddr.to_string(),
            consensus_bootstrap_peer_multiaddr_is_none,
            consensus_secret_key: consensus_secret_key.to_string(),
            mempool_bootstrap_peer_multiaddr: mempool_bootstrap_peer_multiaddr.to_string(),
            mempool_bootstrap_peer_multiaddr_is_none,
            mempool_secret_key: mempool_secret_key.to_string(),
            validator_id: validator_id.to_string(),
        }
    }
}
