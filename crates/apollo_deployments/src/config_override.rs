use std::path::Path;

use apollo_infra_utils::dumping::serialize_to_file;
use serde::Serialize;
use serde_json::to_value;

use crate::deployment::PragmaDomain;

const DEPLOYMENT_FILE_NAME: &str = "deployment_config_override.json";
const INSTANCE_FILE_NAME: &str = "instance_config_override.json";

const PRAGMA_URL_TEMPLATE: &str =
    "https://api.{}.pragma.build/node/v1/data/eth/strk?interval=15min&aggregation=median";

#[derive(Debug, Serialize)]
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

    pub fn create(&self, application_config_subdir: &Path) -> Vec<String> {
        serialize_to_file(
            to_value(&self.deployment_config_override).unwrap(),
            application_config_subdir.join(DEPLOYMENT_FILE_NAME).to_str().unwrap(),
        );

        serialize_to_file(
            to_value(&self.instance_config_override).unwrap(),
            application_config_subdir.join(INSTANCE_FILE_NAME).to_str().unwrap(),
        );
        vec![DEPLOYMENT_FILE_NAME.to_string(), INSTANCE_FILE_NAME.to_string()]
    }
}

#[derive(Debug, Serialize)]
pub struct DeploymentConfigOverride {
    #[serde(rename = "base_layer_config.starknet_contract_address")]
    starknet_contract_address: String,
    chain_id: String,
    eth_fee_token_address: String,
    starknet_url: String,
    strk_fee_token_address: String,
    #[serde(rename = "consensus_manager_config.eth_to_strk_oracle_config.base_url")]
    consensus_manager_config_eth_to_strk_oracle_config_base_url: String,
}

impl DeploymentConfigOverride {
    pub fn new(
        starknet_contract_address: impl ToString,
        chain_id: impl ToString,
        eth_fee_token_address: impl ToString,
        starknet_url: impl ToString,
        strk_fee_token_address: impl ToString,
        pragma_domain: PragmaDomain,
    ) -> Self {
        Self {
            starknet_contract_address: starknet_contract_address.to_string(),
            chain_id: chain_id.to_string(),
            eth_fee_token_address: eth_fee_token_address.to_string(),
            starknet_url: starknet_url.to_string(),
            strk_fee_token_address: strk_fee_token_address.to_string(),
            //TODO(Tsabary): use `format!` instead.
            consensus_manager_config_eth_to_strk_oracle_config_base_url: PRAGMA_URL_TEMPLATE
                .replace("{}", &pragma_domain.to_string()),
        }
    }
}

// TODO(Tsabary): re-verify all config diffs.

#[derive(Debug, Serialize)]
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
    #[serde(flatten)]
    deployment_type_config_override: DeploymentTypeConfigOverride,
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
        deployment_type_config_override: DeploymentTypeConfigOverride,
    ) -> Self {
        Self {
            consensus_bootstrap_peer_multiaddr: consensus_bootstrap_peer_multiaddr.to_string(),
            consensus_bootstrap_peer_multiaddr_is_none,
            consensus_secret_key: consensus_secret_key.to_string(),
            mempool_bootstrap_peer_multiaddr: mempool_bootstrap_peer_multiaddr.to_string(),
            mempool_bootstrap_peer_multiaddr_is_none,
            mempool_secret_key: mempool_secret_key.to_string(),
            validator_id: validator_id.to_string(),
            deployment_type_config_override,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DeploymentTypeConfigOverride {
    #[serde(rename = "l1_scraper_config.startup_rewind_time_seconds")]
    l1_scraper_config_startup_rewind_time_seconds: u64,
    #[serde(rename = "mempool_config.transaction_ttl")]
    mempool_config_transaction_ttl: u64,
}

impl DeploymentTypeConfigOverride {
    pub fn new(
        l1_scraper_config_startup_rewind_time_seconds: u64,
        mempool_config_transaction_ttl: u64,
    ) -> Self {
        Self { l1_scraper_config_startup_rewind_time_seconds, mempool_config_transaction_ttl }
    }
}
