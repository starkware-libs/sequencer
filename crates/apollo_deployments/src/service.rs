use std::collections::{BTreeSet, HashSet};
use std::fmt::Display;

use serde::{Serialize, Serializer};
use strum::{Display, EnumDiscriminants, EnumIter, IntoEnumIterator, IntoStaticStr, VariantNames};

use crate::deployment_definitions::ComponentConfigInService;
use crate::deployments::consolidated::ConsolidatedNodeServiceName;
use crate::deployments::distributed::DistributedNodeServiceName;
use crate::deployments::hybrid::HybridNodeServiceName;
<<<<<<< HEAD
||||||| 01e4b43fa6
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
    "batcher_config.dynamic_config.n_concurrent_txs",
    "batcher_config.dynamic_config.proposer_idle_detection_delay_millis",
    "batcher_config.dynamic_config.storage_access_filter_config.blocked_storage_keys",
    "batcher_config.dynamic_config.storage_access_filter_config.exempt_account_address",
    "batcher_config.dynamic_config.storage_access_filter_config.exempt_account_address.#is_none",
    "batcher_config.static_config.block_builder_config.bouncer_config.block_max_capacity.n_events",
    "batcher_config.static_config.block_builder_config.bouncer_config.block_max_capacity.receipt_l2_gas",
    "batcher_config.static_config.block_builder_config.bouncer_config.block_max_capacity.state_diff_size",
    "batcher_config.static_config.block_builder_config.execute_config.n_workers",
    "batcher_config.static_config.first_block_with_partial_block_hash.#is_none",
    "batcher_config.static_config.first_block_with_partial_block_hash.block_hash",
    "batcher_config.static_config.first_block_with_partial_block_hash.block_number",
    "batcher_config.static_config.first_block_with_partial_block_hash.parent_block_hash",
    "chain_id",
    "class_manager_config.static_config.class_manager_config.max_compiled_contract_class_object_size",
    "committer_config.storage_config.cache_size",
    "committer_config.storage_config.inner_storage_config.cache_size",
    "committer_config.verify_state_diff_hash",
    "consensus_manager_config.consensus_manager_config.dynamic_config.require_virtual_proposer_vote",
    "consensus_manager_config.consensus_manager_config.dynamic_config.timeouts.proposal.base",
    "consensus_manager_config.consensus_manager_config.dynamic_config.timeouts.proposal.max",
    "consensus_manager_config.context_config.dynamic_config.build_proposal_margin_millis",
    "consensus_manager_config.context_config.dynamic_config.compare_retrospective_block_hash",
    "consensus_manager_config.context_config.dynamic_config.min_l2_gas_price_per_height",
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
    "consensus_manager_config.staking_manager_config.dynamic_config.override_committee.#is_none",
    "consensus_manager_config.staking_manager_config.dynamic_config.override_committee",
    "eth_fee_token_address",
    "gateway_config.static_config.authorized_declarer_accounts.#is_none",
    "gateway_config.static_config.authorized_declarer_accounts",
    "gateway_config.static_config.proof_archive_writer_config.bucket_name",
    "gateway_config.static_config.stateful_tx_validator_config.max_allowed_nonce_gap",
    "gateway_config.static_config.stateless_tx_validator_config.max_contract_bytecode_size",
    "gateway_config.static_config.stateless_tx_validator_config.min_gas_price",
    "http_server_config.static_config.port",
    "l1_gas_price_provider_config.chainlink_oracle_config.eth_usd_feed_address",
    "l1_gas_price_provider_config.chainlink_oracle_config.strk_usd_feed_address",
    "l1_gas_price_provider_config.eth_to_strk_oracle_source",
    "l1_gas_price_provider_config.strk_to_usd_oracle_source",
    "mempool_config.dynamic_config.transaction_ttl",
    "mempool_p2p_config.network_config.advertised_multiaddr.#is_none",
    "mempool_p2p_config.network_config.advertised_multiaddr",
    "mempool_p2p_config.network_config.bootstrap_peer_multiaddr.#is_none",
    "mempool_p2p_config.network_config.bootstrap_peer_multiaddr",
    "mempool_p2p_config.network_config.port",
    "monitoring_endpoint_config.port",
    "native_classes_whitelist",
    "recorder_url",
    "sierra_compiler_config.allowed_libfuncs_list",
    "sierra_compiler_config.max_bytecode_size",
    "starknet_url",
    "state_sync_config.static_config.central_sync_client_config.#is_none",
    "state_sync_config.static_config.central_sync_client_config.sync_config.store_sierras_and_casms_block_threshold",
    "state_sync_config.static_config.network_config.#is_none",
    "state_sync_config.static_config.p2p_sync_client_config.#is_none",
    "state_sync_config.static_config.rpc_config.port",
    "strk_fee_token_address",
    "validator_id",
    "versioned_constants_overrides.#is_none",
    "versioned_constants_overrides.max_n_events",
};
=======
use crate::replacers::insert_replacer_annotations;
use crate::scale_policy::ScalePolicy;
#[cfg(test)]
use crate::test_utils::FIX_BINARY_NAME;

const SERVICES_DIR_NAME: &str = "services/";
const REMOTE_SERVICE_URL_PLACEHOLDER: &str = "remote_service";
// Body limits for traffic to and from the committer (the live mainnet values).
pub(crate) const COMMITTER_MAX_REQUEST_BODY_BYTES: usize = 16 * 1024 * 1024;
pub(crate) const COMMITTER_MAX_RESPONSE_BODY_BYTES: usize = 64 * 1024 * 1024;

// TODO(Tsabary): remove ports and mempool ttl from this list.
pub static KEYS_TO_BE_REPLACED: phf::Set<&'static str> = phf_set! {
    "base_layer_config.bpo1_start_block_number",
    "base_layer_config.bpo2_start_block_number",
    "base_layer_config.fusaka_no_bpo_start_block_number",
    "base_layer_config.starknet_contract_address",
    "batcher_config.dynamic_config.n_concurrent_txs",
    "batcher_config.dynamic_config.proposer_idle_detection_delay_millis",
    "batcher_config.dynamic_config.storage_access_filter_config.blocked_storage_keys",
    "batcher_config.dynamic_config.storage_access_filter_config.exempt_account_address",
    "batcher_config.dynamic_config.storage_access_filter_config.exempt_account_address.#is_none",
    "batcher_config.static_config.block_builder_config.bouncer_config.block_max_capacity.n_events",
    "batcher_config.static_config.block_builder_config.bouncer_config.block_max_capacity.receipt_l2_gas",
    "batcher_config.static_config.block_builder_config.bouncer_config.block_max_capacity.state_diff_size",
    "batcher_config.static_config.block_builder_config.execute_config.n_workers",
    "batcher_config.static_config.contract_class_manager_config.cairo_native_run_config.cairo_native_mode",
    "batcher_config.static_config.first_block_with_partial_block_hash.#is_none",
    "batcher_config.static_config.first_block_with_partial_block_hash.block_hash",
    "batcher_config.static_config.first_block_with_partial_block_hash.block_number",
    "batcher_config.static_config.first_block_with_partial_block_hash.parent_block_hash",
    "chain_id",
    "class_manager_config.static_config.class_manager_config.max_compiled_contract_class_object_size",
    "committer_config.storage_config.cache_size",
    "committer_config.storage_config.inner_storage_config.cache_size",
    "committer_config.verify_state_diff_hash",
    "consensus_manager_config.consensus_manager_config.dynamic_config.require_virtual_proposer_vote",
    "consensus_manager_config.consensus_manager_config.dynamic_config.timeouts.proposal.base",
    "consensus_manager_config.consensus_manager_config.dynamic_config.timeouts.proposal.max",
    "consensus_manager_config.context_config.dynamic_config.build_proposal_margin_millis",
    "consensus_manager_config.context_config.dynamic_config.compare_retrospective_block_hash",
    "consensus_manager_config.context_config.dynamic_config.min_l2_gas_price_per_height",
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
    "consensus_manager_config.staking_manager_config.dynamic_config.override_committee.#is_none",
    "consensus_manager_config.staking_manager_config.dynamic_config.override_committee",
    "eth_fee_token_address",
    "gateway_config.static_config.authorized_declarer_accounts.#is_none",
    "gateway_config.static_config.authorized_declarer_accounts",
    "gateway_config.static_config.contract_class_manager_config.cairo_native_run_config.cairo_native_mode",
    "gateway_config.static_config.proof_archive_writer_config.bucket_name",
    "gateway_config.static_config.stateful_tx_validator_config.max_allowed_nonce_gap",
    "gateway_config.static_config.stateless_tx_validator_config.allow_proof_version_v1",
    "gateway_config.static_config.stateless_tx_validator_config.allow_proof_version_v2",
    "gateway_config.static_config.stateless_tx_validator_config.max_contract_bytecode_size",
    "gateway_config.static_config.stateless_tx_validator_config.min_gas_price",
    "http_server_config.static_config.port",
    "l1_gas_price_provider_config.chainlink_oracle_config.eth_usd_feed_address",
    "l1_gas_price_provider_config.chainlink_oracle_config.strk_usd_feed_address",
    "l1_gas_price_provider_config.eth_to_strk_oracle_source",
    "l1_gas_price_provider_config.strk_to_usd_oracle_source",
    "mempool_config.dynamic_config.transaction_ttl",
    "mempool_p2p_config.network_config.advertised_multiaddr.#is_none",
    "mempool_p2p_config.network_config.advertised_multiaddr",
    "mempool_p2p_config.network_config.bootstrap_peer_multiaddr.#is_none",
    "mempool_p2p_config.network_config.bootstrap_peer_multiaddr",
    "mempool_p2p_config.network_config.port",
    "monitoring_endpoint_config.port",
    "native_classes_whitelist",
    "recorder_url",
    "sierra_compiler_config.allowed_libfuncs_list",
    "sierra_compiler_config.max_bytecode_size",
    "starknet_url",
    "state_sync_config.static_config.central_sync_client_config.#is_none",
    "state_sync_config.static_config.central_sync_client_config.sync_config.store_sierras_and_casms_block_threshold",
    "state_sync_config.static_config.network_config.#is_none",
    "state_sync_config.static_config.p2p_sync_client_config.#is_none",
    "state_sync_config.static_config.rpc_config.port",
    "strk_fee_token_address",
    "validator_id",
    "versioned_constants_overrides.#is_none",
    "versioned_constants_overrides.max_n_events",
};
>>>>>>> origin/main-v0.14.4

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumDiscriminants)]
#[strum_discriminants(
    name(NodeType),
    derive(IntoStaticStr, EnumIter, VariantNames, Serialize, Display),
    strum(serialize_all = "snake_case")
)]
pub enum NodeService {
    Consolidated(ConsolidatedNodeServiceName),
    Hybrid(HybridNodeServiceName),
    Distributed(DistributedNodeServiceName),
}

impl NodeService {
    fn as_inner(&self) -> &dyn ServiceNameInner {
        match self {
            NodeService::Consolidated(inner) => inner,
            NodeService::Hybrid(inner) => inner,
            NodeService::Distributed(inner) => inner,
        }
    }

    pub fn get_components_in_service(&self) -> BTreeSet<ComponentConfigInService> {
        self.as_inner().get_components_in_service()
    }
}

pub(crate) trait ServiceNameInner: Display {
    fn get_components_in_service(&self) -> BTreeSet<ComponentConfigInService>;
}

impl NodeType {
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
<<<<<<< HEAD
||||||| 01e4b43fa6

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
    let port_cond = key.ends_with(".port") && value.as_u64().is_some_and(|n| n != invalid_port);

    // Condition 2: service urls: ".url" suffix and a non-localhost string value
    let url_cond = key.ends_with(".url") && value.as_str().is_some_and(|s| s != DEFAULT_URL);

    port_cond || url_cond
}

pub(crate) trait GetComponentConfigs: ServiceNameInner {
    fn get_component_configs(ports: Option<Vec<u16>>) -> HashMap<NodeService, ComponentConfig>;

    /// Returns a component execution config for a component that runs locally, and accepts inbound
    /// connections from remote components.
    fn component_config_for_local_service(&self, port: u16) -> ReactiveComponentExecutionConfig {
        ReactiveComponentExecutionConfig::local_with_remote_enabled(
            REMOTE_SERVICE_URL_PLACEHOLDER.to_string(),
            port,
        )
    }

    /// Returns a component execution config for a component that is accessed remotely.
    fn component_config_for_remote_service(&self, port: u16) -> ReactiveComponentExecutionConfig {
        let idle_connections = self.get_scale_policy().idle_connections();
        let retries = self.get_retries();
        ReactiveComponentExecutionConfig::remote(REMOTE_SERVICE_URL_PLACEHOLDER.to_string(), port)
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
=======

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
    let port_cond = key.ends_with(".port") && value.as_u64().is_some_and(|n| n != invalid_port);

    // Condition 2: service urls: ".url" suffix and a non-localhost string value
    let url_cond = key.ends_with(".url") && value.as_str().is_some_and(|s| s != DEFAULT_URL);

    port_cond || url_cond
}

pub(crate) trait GetComponentConfigs: ServiceNameInner {
    fn get_component_configs(ports: Option<Vec<u16>>) -> HashMap<NodeService, ComponentConfig>;

    /// Returns a component execution config for a component that runs locally, and accepts inbound
    /// connections from remote components.
    fn component_config_for_local_service(&self, port: u16) -> ReactiveComponentExecutionConfig {
        ReactiveComponentExecutionConfig::local_with_remote_enabled(
            REMOTE_SERVICE_URL_PLACEHOLDER.to_string(),
            port,
        )
    }

    /// Returns a component execution config for a component that is accessed remotely.
    fn component_config_for_remote_service(&self, port: u16) -> ReactiveComponentExecutionConfig {
        let idle_connections = self.get_scale_policy().idle_connections();
        let retries = self.get_retries();
        ReactiveComponentExecutionConfig::remote(REMOTE_SERVICE_URL_PLACEHOLDER.to_string(), port)
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

    /// Sets the largest request the component's server accepts and the largest response its
    /// clients accept.
    pub(crate) fn with_max_body_bytes(
        mut self,
        max_request_body_bytes: usize,
        max_response_body_bytes: usize,
    ) -> Self {
        self.local = self.local.with_max_request_body_bytes(max_request_body_bytes);
        self.remote = self.remote.with_max_response_body_bytes(max_response_body_bytes);
        self
    }
>>>>>>> origin/main-v0.14.4
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
