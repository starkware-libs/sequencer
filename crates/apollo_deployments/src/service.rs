use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Display;

use apollo_node_config::component_config::ComponentConfig;
use apollo_node_config::component_execution_config::ReactiveComponentExecutionConfig;
use phf::phf_set;
use serde::{Serialize, Serializer};
use strum::{Display, EnumDiscriminants, EnumIter, IntoEnumIterator, IntoStaticStr, VariantNames};

use crate::deployment_definitions::ComponentConfigInService;
use crate::deployments::consolidated::ConsolidatedNodeServiceName;
use crate::deployments::distributed::DistributedNodeServiceName;
use crate::deployments::hybrid::HybridNodeServiceName;
use crate::scale_policy::ScalePolicy;

const REMOTE_SERVICE_URL_PLACEHOLDER: &str = "remote_service";

// The non-overridable config keys that the applicative-vs-`app_configs` guard
// (`jsonnet::test_applicative_matches_app_configs`) excludes from its comparison: deploy-time
// overridable values that the jsonnet applicative layer does not bake in.
// TODO(Tsabary): remove ports and mempool ttl from this list.
pub static KEYS_TO_BE_REPLACED: phf::Set<&'static str> = phf_set! {
    "base_layer_config.bpo1_start_block_number",
    "base_layer_config.bpo2_start_block_number",
    "base_layer_config.fusaka_no_bpo_start_block_number",
    "base_layer_config.starknet_contract_address",
    "batcher_config.dynamic_config.n_concurrent_txs",
    "batcher_config.dynamic_config.proposer_idle_detection_delay_millis",
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
    "state_sync_config.static_config.central_sync_client_config.sync_config.store_sierras_and_casms_block_threshold",
    "state_sync_config.static_config.network_config.#is_none",
    "state_sync_config.static_config.network_config.port",
    "state_sync_config.static_config.p2p_sync_client_config.#is_none",
    "state_sync_config.static_config.rpc_config.port",
    "strk_fee_token_address",
    "validator_id",
    "versioned_constants_overrides.#is_none",
    "versioned_constants_overrides.max_n_events",
};

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
    fn get_scale_policy(&self) -> ScalePolicy;

    fn get_retries(&self) -> usize;

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
