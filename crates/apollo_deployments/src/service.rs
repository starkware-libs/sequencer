use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Display;
use std::fs::File;
use std::io::Read;
use std::iter::once;
use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};

use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam, FIELD_SEPARATOR, IS_NONE_MARK};
use apollo_infra_utils::dumping::serialize_to_file;
#[cfg(test)]
use apollo_infra_utils::dumping::serialize_to_file_test;
use apollo_node_config::component_config::ComponentConfig;
use apollo_node_config::component_execution_config::{
    ReactiveComponentExecutionConfig,
    DEFAULT_INVALID_PORT,
    DEFAULT_URL,
};
use apollo_node_config::config_utils::{config_to_preset, prune_by_is_none};
use phf::phf_set;
use serde::{Serialize, Serializer};
use serde_json::{from_str, json, Map, Value};
use strum::{Display, EnumVariantNames, IntoEnumIterator};
use strum_macros::{EnumDiscriminants, EnumIter, IntoStaticStr};

use crate::deployment_definitions::{ComponentConfigInService, CONFIG_BASE_DIR};
use crate::deployments::consolidated::ConsolidatedNodeServiceName;
use crate::deployments::distributed::DistributedNodeServiceName;
use crate::deployments::hybrid::HybridNodeServiceName;
use crate::replacers::insert_replacer_annotations;
use crate::scale_policy::ScalePolicy;
#[cfg(test)]
use crate::test_utils::FIX_BINARY_NAME;

const SERVICES_DIR_NAME: &str = "services/";
const REMOTE_SERVICE_URL_PLACEHOLDER: &str = "remote_service";

// TODO(Tsabary): remove ports and mempool ttl from this list.
pub static KEYS_TO_BE_REPLACED: phf::Set<&'static str> = phf_set! {
    "base_layer_config.bpo1_start_block_number",
    "base_layer_config.bpo2_start_block_number",
    "base_layer_config.fusaka_no_bpo_start_block_number",
    "base_layer_config.starknet_contract_address",
    "batcher_config.static_config.block_builder_config.bouncer_config.block_max_capacity.n_events",
    "batcher_config.static_config.block_builder_config.bouncer_config.block_max_capacity.state_diff_size",
    "batcher_config.static_config.block_builder_config.execute_config.n_workers",
    "batcher_config.static_config.block_builder_config.proposer_idle_detection_delay_millis",
    "batcher_config.static_config.first_block_with_partial_block_hash.#is_none",
    "batcher_config.static_config.first_block_with_partial_block_hash.block_number",
    "batcher_config.static_config.first_block_with_partial_block_hash.block_hash",
    "batcher_config.static_config.first_block_with_partial_block_hash.parent_block_hash",
    "batcher_config.static_config.contract_class_manager_config.native_compiler_config.max_cpu_time",
    "chain_id",
    "class_manager_config.static_config.class_manager_config.max_compiled_contract_class_object_size",
    "committer_config.storage_config.cache_size",
    "committer_config.verify_state_diff_hash",
    "consensus_manager_config.consensus_manager_config.dynamic_config.require_virtual_proposer_vote",
    "consensus_manager_config.consensus_manager_config.dynamic_config.timeouts.proposal.base",
    "consensus_manager_config.consensus_manager_config.dynamic_config.timeouts.proposal.max",
    "consensus_manager_config.context_config.static_config.build_proposal_margin_millis",
    "consensus_manager_config.context_config.dynamic_config.override_eth_to_fri_rate.#is_none",
    "consensus_manager_config.context_config.dynamic_config.override_eth_to_fri_rate",
    "consensus_manager_config.context_config.dynamic_config.override_l1_data_gas_price_fri.#is_none",
    "consensus_manager_config.context_config.dynamic_config.override_l1_data_gas_price_fri",
    "consensus_manager_config.context_config.dynamic_config.override_l1_gas_price_fri.#is_none",
    "consensus_manager_config.context_config.dynamic_config.override_l1_gas_price_fri",
    "consensus_manager_config.context_config.dynamic_config.override_l2_gas_price_fri.#is_none",
    "consensus_manager_config.context_config.dynamic_config.override_l2_gas_price_fri",
    "consensus_manager_config.network_config.advertised_multiaddr.#is_none",
    "consensus_manager_config.network_config.advertised_multiaddr",
    "consensus_manager_config.network_config.bootstrap_peer_multiaddr.#is_none",
    "consensus_manager_config.network_config.bootstrap_peer_multiaddr",
    "consensus_manager_config.network_config.port",
    "consensus_manager_config.staking_manager_config.dynamic_config.default_committee",
    "eth_fee_token_address",
    "gateway_config.static_config.authorized_declarer_accounts.#is_none",
    "gateway_config.static_config.authorized_declarer_accounts",
    "gateway_config.static_config.contract_class_manager_config.native_compiler_config.max_cpu_time",
    "gateway_config.static_config.stateful_tx_validator_config.max_allowed_nonce_gap",
    "gateway_config.static_config.stateless_tx_validator_config.min_gas_price",
    "http_server_config.static_config.port",
    "mempool_config.dynamic_config.transaction_ttl",
    "mempool_p2p_config.network_config.advertised_multiaddr.#is_none",
    "mempool_p2p_config.network_config.advertised_multiaddr",
    "mempool_p2p_config.network_config.bootstrap_peer_multiaddr.#is_none",
    "mempool_p2p_config.network_config.bootstrap_peer_multiaddr",
    "mempool_p2p_config.network_config.port",
    "monitoring_endpoint_config.port",
    "native_classes_whitelist",
    "recorder_url",
    "sierra_compiler_config.audited_libfuncs_only",
    "sierra_compiler_config.max_bytecode_size",
    "starknet_url",
    "state_sync_config.static_config.central_sync_client_config.#is_none",
    "state_sync_config.static_config.network_config.#is_none",
    "state_sync_config.static_config.p2p_sync_client_config.#is_none",
    "state_sync_config.static_config.rpc_config.port",
    "strk_fee_token_address",
    "validator_id",
    "versioned_constants_overrides.max_n_events",
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumDiscriminants)]
#[strum_discriminants(
    name(NodeType),
    derive(IntoStaticStr, EnumIter, EnumVariantNames, Serialize, Display),
    strum(serialize_all = "snake_case")
)]
pub enum NodeService {
    Consolidated(ConsolidatedNodeServiceName),
    Hybrid(HybridNodeServiceName),
    Distributed(DistributedNodeServiceName),
}

// TODO(Tsabary): move p2p ports from the application configs to the replacer format.

impl NodeService {
    pub fn replacer_deployment_file_path(&self) -> String {
        PathBuf::from(CONFIG_BASE_DIR)
            .join(SERVICES_DIR_NAME)
            .join(NodeType::from(self).get_folder_name())
            .join(format!("replacer_deployment_{}.json", self.as_inner()))
            .to_string_lossy()
            .to_string()
    }

    fn get_replacer_config_file_path(&self) -> String {
        format!("replacer_{}.json", self.as_inner())
    }

    fn as_inner(&self) -> &dyn ServiceNameInner {
        match self {
            NodeService::Consolidated(inner) => inner,
            NodeService::Hybrid(inner) => inner,
            NodeService::Distributed(inner) => inner,
        }
    }

    fn get_replacer_service_file_path(&self) -> String {
        PathBuf::from(CONFIG_BASE_DIR)
            .join(SERVICES_DIR_NAME)
            .join(NodeType::from(self).get_folder_name())
            .join(self.get_replacer_config_file_path())
            .to_string_lossy()
            .to_string()
    }

    pub fn get_components_in_service(&self) -> BTreeSet<ComponentConfigInService> {
        self.as_inner().get_components_in_service()
    }

    fn replacer_app_config_files(&self) -> Vec<(Value, String)> {
        let components_in_service = self
            .get_components_in_service()
            .into_iter()
            .flat_map(|c| c.get_component_config_file_paths())
            .collect::<Vec<_>>();

        let replacer_components_in_service = self
            .get_components_in_service()
            .into_iter()
            .flat_map(|c| c.get_replacer_component_config_file_paths())
            .collect::<Vec<_>>();

        let replacer_app_config_data: Vec<Value> = components_in_service
            .iter()
            .map(|src| {
                let src_path = Path::new(src);
                // Read the app config file
                let mut contents = String::new();
                File::open(src_path).unwrap().read_to_string(&mut contents).unwrap();

                // Parse it as a json
                let map: Map<String, Value> =
                    from_str(&contents).expect("JSON should be an object");
                let original_app_config = Value::Object(map);

                // Perform replacement
                insert_replacer_annotations(original_app_config, replace_pred)
            })
            .collect();

        let mut data_and_file_paths: Vec<(Value, String)> = replacer_app_config_data
            .into_iter()
            .zip(replacer_components_in_service.clone())
            .collect();

        let replacer_config_paths: Vec<String> = replacer_components_in_service
            .into_iter()
            .chain(once(self.get_replacer_service_file_path()))
            .collect();
        let replacer_deployment_file_path = self.replacer_deployment_file_path();

        data_and_file_paths.push((replacer_config_paths.into(), replacer_deployment_file_path));

        data_and_file_paths
    }

    pub fn dump_node_service_replacer_app_config_files(&self) {
        for (data, file_path) in self.replacer_app_config_files().into_iter() {
            serialize_to_file(&data, &file_path);
        }
    }

    #[cfg(test)]
    pub fn test_dump_node_service_replacer_app_config_files(&self) {
        for (data, file_path) in self.replacer_app_config_files().into_iter() {
            serialize_to_file_test(&data, &file_path, FIX_BINARY_NAME);
        }
    }
}

pub(crate) trait ServiceNameInner: Display {
    fn get_scale_policy(&self) -> ScalePolicy;

    fn get_retries(&self) -> usize;

    fn get_components_in_service(&self) -> BTreeSet<ComponentConfigInService>;
}

impl NodeType {
    fn get_folder_name(&self) -> String {
        self.to_string()
    }

    pub fn all_service_names(&self) -> Vec<NodeService> {
        match self {
            // TODO(Tsabary): find a way to avoid this code duplication.
            Self::Consolidated => {
                ConsolidatedNodeServiceName::iter().map(NodeService::Consolidated).collect()
            }
            Self::Hybrid => HybridNodeServiceName::iter().map(NodeService::Hybrid).collect(),
            Self::Distributed => {
                DistributedNodeServiceName::iter().map(NodeService::Distributed).collect()
            }
        }
    }

    pub fn get_services_of_components(
        &self,
        component_type: ComponentConfigInService,
    ) -> HashSet<NodeService> {
        let services: HashSet<_> = self
            .all_service_names()
            .into_iter()
            .filter(|node_service| {
                node_service.get_components_in_service().contains(&component_type)
            })
            .collect();

        assert!(
            !services.is_empty(),
            "Expected at least one NodeService containing component type {:?}",
            component_type
        );

        services
    }

    pub fn get_component_configs(
        &self,
        ports: Option<Vec<u16>>,
    ) -> HashMap<NodeService, ComponentConfig> {
        match self {
            // TODO(Tsabary): avoid this code duplication.
            Self::Consolidated => ConsolidatedNodeServiceName::get_component_configs(ports),
            Self::Hybrid => HybridNodeServiceName::get_component_configs(ports),
            Self::Distributed => DistributedNodeServiceName::get_component_configs(ports),
        }
    }

    fn dump_component_configs_with<SerdeFn>(&self, ports: Option<Vec<u16>>, writer: SerdeFn)
    where
        SerdeFn: Fn(&serde_json::Value, &str),
    {
        let component_configs = self.get_component_configs(ports);
        for (node_service, component_config) in component_configs {
            let components_in_service = node_service.get_components_in_service();
            let wrapper =
                ComponentConfigsSerializationWrapper::new(component_config, components_in_service);
            let flattened = config_to_preset(&json!(wrapper.dump()));
            let pruned = prune_by_is_none(flattened);

            // Dumping in the replacer format.
            let pruned_with_replacer_annotations =
                insert_replacer_annotations(pruned, replace_pred);
            let file_path = node_service.get_replacer_service_file_path();
            writer(&pruned_with_replacer_annotations, &file_path);
        }
    }

    pub fn dump_service_component_configs(&self, ports: Option<Vec<u16>>) {
        self.dump_component_configs_with(ports, |map, path| {
            serialize_to_file(map, path);
        });
    }

    #[cfg(test)]
    pub fn test_dump_service_component_configs(&self, ports: Option<Vec<u16>>) {
        self.dump_component_configs_with(ports, |map, path| {
            serialize_to_file_test(map, path, FIX_BINARY_NAME);
        });
    }

    #[cfg(test)]
    pub fn test_all_replacers_are_accounted_for(&self) {
        // Obtain the application config keys of each service.
        let application_config_keys: HashSet<String> = self
            .all_service_names()
            .iter()
            .flat_map(|node_service| {
                // TODO(Tsabary): consider wrapping this logic with a fn; more relevant once we're
                // done transitioning to the new deployment mechanism.
                node_service
                    .get_components_in_service()
                    .into_iter()
                    .flat_map(|c| c.get_component_config_file_paths())
                    .collect::<HashSet<_>>()
                    .iter()
                    .flat_map(|src| {
                        let src_path = Path::new(src);
                        // Read the app config file
                        let mut contents = String::new();
                        File::open(src_path).unwrap().read_to_string(&mut contents).unwrap();

                        // Extract keys
                        from_str::<Map<String, Value>>(&contents)
                            .expect("JSON should be an object")
                            .into_iter()
                            .map(|(k, _)| k)
                            .collect::<HashSet<_>>()
                    })
                    .collect::<HashSet<_>>()
            })
            .collect::<HashSet<_>>();

        let replacer_keys: HashSet<String> =
            KEYS_TO_BE_REPLACED.iter().copied().map(|item| item.to_string()).collect();

        let unreplaced_keys: HashSet<String> =
            replacer_keys.difference(&application_config_keys).cloned().collect();

        assert!(
            unreplaced_keys.is_empty(),
            "Some replacer keys are not part of the config: {unreplaced_keys:#?}
            \nPlease update 'KEYS_TO_BE_REPLACED'"
        );
    }
}

fn replace_pred(key: &str, value: &Value) -> bool {
    if KEYS_TO_BE_REPLACED.contains(key) {
        return true;
    }

    let invalid_port: u64 = DEFAULT_INVALID_PORT.into();

    // Condition 1: ports set by the infra: ".port" suffix and a non-zero integer value
    let port_cond =
        key.ends_with(".port") && value.as_u64().map(|n| n != invalid_port).unwrap_or(false);

    // Condition 2: service urls: ".url" suffix and a non-localhost string value
    let url_cond =
        key.ends_with(".url") && value.as_str().map(|s| s != DEFAULT_URL).unwrap_or(false);

    port_cond || url_cond
}

pub(crate) trait GetComponentConfigs: ServiceNameInner {
    fn get_component_configs(ports: Option<Vec<u16>>) -> HashMap<NodeService, ComponentConfig>;

    /// Returns a component execution config for a component that runs locally, and accepts inbound
    /// connections from remote components.
    fn component_config_for_local_service(&self, port: u16) -> ReactiveComponentExecutionConfig {
        ReactiveComponentExecutionConfig::local_with_remote_enabled(
            REMOTE_SERVICE_URL_PLACEHOLDER.to_string(),
            IpAddr::from(Ipv4Addr::UNSPECIFIED),
            port,
        )
    }

    /// Returns a component execution config for a component that is accessed remotely.
    fn component_config_for_remote_service(&self, port: u16) -> ReactiveComponentExecutionConfig {
        let idle_connections = self.get_scale_policy().idle_connections();
        let retries = self.get_retries();
        ReactiveComponentExecutionConfig::remote(
            REMOTE_SERVICE_URL_PLACEHOLDER.to_string(),
            IpAddr::from(Ipv4Addr::UNSPECIFIED),
            port,
        )
        .with_idle_connections(idle_connections)
        .with_retries(retries)
    }

    fn component_config_pair(&self, port: u16) -> ComponentConfigPair {
        ComponentConfigPair {
            local: self.component_config_for_local_service(port),
            remote: self.component_config_for_remote_service(port),
        }
    }
}

/// Component config bundling for node services: a config to run a component
/// locally while being accessible to other remote components, and a suitable remote-access config
/// to be used by such remotes.
pub(crate) struct ComponentConfigPair {
    local: ReactiveComponentExecutionConfig,
    remote: ReactiveComponentExecutionConfig,
}

impl ComponentConfigPair {
    pub(crate) fn local(&self) -> ReactiveComponentExecutionConfig {
        self.local.clone()
    }

    pub(crate) fn remote(&self) -> ReactiveComponentExecutionConfig {
        self.remote.clone()
    }
}

impl Serialize for NodeService {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Serialize only the inner value.
        match self {
            NodeService::Consolidated(inner) => inner.serialize(serializer),
            NodeService::Hybrid(inner) => inner.serialize(serializer),
            NodeService::Distributed(inner) => inner.serialize(serializer),
        }
    }
}

// A helper struct for serializing the components config in the same hierarchy as of its
// serialization as part of the entire config, i.e., by prepending "components.".
#[derive(Clone, Debug, Default, Serialize)]
struct ComponentConfigsSerializationWrapper {
    component_config: ComponentConfig,
    components_in_service: BTreeSet<ComponentConfigInService>,
}

impl ComponentConfigsSerializationWrapper {
    fn new(
        component_config: ComponentConfig,
        components_in_service: BTreeSet<ComponentConfigInService>,
    ) -> Self {
        ComponentConfigsSerializationWrapper { component_config, components_in_service }
    }
}

impl SerializeConfig for ComponentConfigsSerializationWrapper {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut map = prepend_sub_config_name(self.component_config.dump(), "components");
        for component_config_in_service in ComponentConfigInService::iter() {
            if component_config_in_service == ComponentConfigInService::General {
                // General configs are not toggle-able, i.e., no need to add their existence to the
                // service config.
                continue;
            }
            let component_config_names = component_config_in_service.get_component_config_names();
            let is_in_service = self.components_in_service.contains(&component_config_in_service);
            for component_config_name in component_config_names {
                let (param_path, serialized_param) = ser_param(
                    &format!("{component_config_name}{FIELD_SEPARATOR}{IS_NONE_MARK}"),
                    &!is_in_service, // Marking the config as None.
                    "Placeholder description",
                    ParamPrivacyInput::Public,
                );
                map.insert(param_path, serialized_param);
            }
        }
        map
    }
}
