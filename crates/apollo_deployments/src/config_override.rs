use std::path::{Path, PathBuf};

use apollo_config::converters::serialize_optional_comma_separated;
use apollo_infra_utils::dumping::serialize_to_file;
#[cfg(test)]
use apollo_infra_utils::dumping::serialize_to_file_test;
use libp2p::Multiaddr;
use serde::{Serialize, Serializer};
use serde_json::to_value;
use serde_with::with_prefix;
use starknet_api::block::BlockNumber;
use url::Url;

use crate::deployment_definitions::{StateSyncConfig, StateSyncType};
use crate::replacers::insert_replacer_annotations;
#[cfg(test)]
use crate::test_utils::FIX_BINARY_NAME;

const DEPLOYMENT_FILE_NAME: &str = "deployment_config_override.json";
const REPLACER_DEPLOYMENT_FILE_NAME: &str = "replacer_deployment.json";
const REPLACER_INSTANCE_FILE_NAME: &str = "replacer_instance.json";
const REPLACER_DIR: &str = "crates/apollo_deployments/resources/deployments/";

// Serialization prefixes for p2p configs
with_prefix!(consensus_prefix "consensus_manager_config.network_config.");
with_prefix!(mempool_prefix "mempool_p2p_config.network_config.");

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct ConfigOverride {
    deployment_config_override: DeploymentConfigOverride,
    instance_config_override: InstanceConfigOverride,
}

pub(crate) fn deployment_replacer_file_path() -> String {
    PathBuf::from(REPLACER_DIR).join(REPLACER_DEPLOYMENT_FILE_NAME).to_string_lossy().to_string()
}

pub(crate) fn instance_replacer_file_path() -> String {
    PathBuf::from(REPLACER_DIR).join(REPLACER_INSTANCE_FILE_NAME).to_string_lossy().to_string()
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
        let instance_path = deployment_config_override_dir.join(format!("{instance_name}.json"));

        if create {
            let deployment_data = to_value(&self.deployment_config_override).unwrap();
            serialize_to_file(&deployment_data, deployment_path.to_str().unwrap());
            serialize_to_file(
                &insert_replacer_annotations(deployment_data, |_, _| true),
                &deployment_replacer_file_path(),
            );

            let instance_data = to_value(&self.instance_config_override).unwrap();
            serialize_to_file(&instance_data, instance_path.to_str().unwrap());
            serialize_to_file(
                &insert_replacer_annotations(instance_data, |_, _| true),
                &instance_replacer_file_path(),
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
            &to_value(config_override_with_paths.deployment_config_override).unwrap(),
            &config_override_with_paths.deployment_path,
            FIX_BINARY_NAME,
        );

        serialize_to_file_test(
            &to_value(config_override_with_paths.instance_config_override).unwrap(),
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
    #[serde(rename = "sierra_compiler_config.audited_libfuncs_only")]
    sierra_compiler_config_audited_libfuncs_only: bool,
    #[serde(flatten)]
    state_sync_config: StateSyncConfig,
    #[serde(flatten, with = "consensus_prefix")]
    consensus_p2p_bootstrap_config: PeerToPeerBootstrapConfig,
    #[serde(flatten, with = "mempool_prefix")]
    mempool_p2p_bootstrap_config: PeerToPeerBootstrapConfig,
    #[serde(rename = "http_server_config.port")]
    http_server_config_port: u16,
    #[serde(rename = "monitoring_endpoint_config.port")]
    monitoring_endpoint_config_port: u16,
    #[serde(rename = "state_sync_config.rpc_config.port")]
    state_sync_config_rpc_config_port: u16,
    #[serde(rename = "mempool_p2p_config.network_config.port")]
    mempool_p2p_config_network_config_port: u16,
    #[serde(rename = "consensus_manager_config.network_config.port")]
    consensus_manager_config_network_config_port: u16,
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
        consensus_p2p_bootstrap_config: PeerToPeerBootstrapConfig,
        mempool_p2p_bootstrap_config: PeerToPeerBootstrapConfig,
        sierra_compiler_config_audited_libfuncs_only: bool,
        http_server_config_port: u16,
        monitoring_endpoint_config_port: u16,
        state_sync_config_rpc_config_port: u16,
        mempool_p2p_config_network_config_port: u16,
        consensus_manager_config_network_config_port: u16, 
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
            sierra_compiler_config_audited_libfuncs_only,
            state_sync_config: state_sync_type.get_state_sync_config(),
            consensus_p2p_bootstrap_config,
            mempool_p2p_bootstrap_config,
            http_server_config_port,
            monitoring_endpoint_config_port,
            state_sync_config_rpc_config_port,
            mempool_p2p_config_network_config_port,
            consensus_manager_config_network_config_port,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct PeerToPeerBootstrapConfig {
    // Bootstrap peer address.
    #[serde(
        rename = "bootstrap_peer_multiaddr",
        serialize_with = "serialize_optional_comma_separated_wrapper"
    )]
    bootstrap_peers_multiaddrs: Option<Vec<Multiaddr>>,
    #[serde(rename = "bootstrap_peer_multiaddr.#is_none")]
    bootstrap_peer_multiaddr_is_none: bool,
}

impl PeerToPeerBootstrapConfig {
    pub fn new(bootstrap_peers_multiaddrs: Option<Vec<Multiaddr>>) -> Self {
        let (bootstrap_peers_multiaddrs, bootstrap_peer_multiaddr_is_none) =
            match bootstrap_peers_multiaddrs {
                Some(addrs) => (Some(addrs), false),
                None => (Some(vec![]), true),
            };
        Self { bootstrap_peers_multiaddrs, bootstrap_peer_multiaddr_is_none }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct PeerToPeerAdvertisementConfig {
    // Advertised self address.
    #[serde(rename = "advertised_multiaddr")]
    advertised_multiaddr: Multiaddr,
    #[serde(rename = "advertised_multiaddr.#is_none")]
    advertised_multiaddr_is_none: bool,
}

impl PeerToPeerAdvertisementConfig {
    pub fn new(advertised_multiaddr: Option<Multiaddr>) -> Self {
        let (advertised_multiaddr, advertised_multiaddr_is_none) = match advertised_multiaddr {
            Some(addr) => (addr, false),
            None => (Multiaddr::empty(), true),
        };
        Self { advertised_multiaddr, advertised_multiaddr_is_none }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct InstanceConfigOverride {
    #[serde(flatten, with = "consensus_prefix")]
    consensus_p2p_advertisement_config: PeerToPeerAdvertisementConfig,
    #[serde(flatten, with = "mempool_prefix")]
    mempool_p2p_advertisement_config: PeerToPeerAdvertisementConfig,
    validator_id: String,
}

impl InstanceConfigOverride {
    pub fn new(
        consensus_p2p_advertisement_config: PeerToPeerAdvertisementConfig,
        mempool_p2p_advertisement_config: PeerToPeerAdvertisementConfig,
        validator_id: impl ToString,
    ) -> Self {
        Self {
            consensus_p2p_advertisement_config,
            mempool_p2p_advertisement_config,
            validator_id: validator_id.to_string(),
        }
    }
}

// Wrapper function for the generic `serialize_optional_comma_separated` function, to be
// compatible with serde's `serialize_with` attribute. It first applies the custom serialization
// logic to convert the optional list into a `String`, and then serializes that string.
fn serialize_optional_comma_separated_wrapper<S, T>(
    optional_list: &Option<Vec<T>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: ToString,
{
    match serialize_optional_comma_separated(optional_list) {
        None => serializer.serialize_none(),
        Some(s) => serializer.serialize_some(&s),
    }
}
