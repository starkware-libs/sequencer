use std::collections::HashSet;
use std::fs;
use std::sync::LazyLock;

use jrsonnet_evaluator::State;
use serde_json::Value;

/// Keys that must be overridden per-environment before a config is valid. Every path listed here
/// must exist in components.libsonnet and have `mustOverride` (i.e. `error "..."`) as its value.
pub(crate) static KEYS_TO_BE_REPLACED: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    HashSet::from([
        "base_layer_config.bpo1_start_block_number",
        "base_layer_config.bpo2_start_block_number",
        "base_layer_config.fusaka_no_bpo_start_block_number",
        "base_layer_config.starknet_contract_address",
        "batcher_config.dynamic_config.n_concurrent_txs",
        "batcher_config.dynamic_config.native_classes_whitelist",
        "batcher_config.dynamic_config.proposer_idle_detection_delay_millis",
        "batcher_config.static_config.block_builder_config.bouncer_config.block_max_capacity.\
         n_events",
        "batcher_config.static_config.block_builder_config.bouncer_config.block_max_capacity.\
         receipt_l2_gas",
        "batcher_config.static_config.block_builder_config.bouncer_config.block_max_capacity.\
         state_diff_size",
        "batcher_config.static_config.block_builder_config.chain_info.chain_id",
        "batcher_config.static_config.block_builder_config.chain_info.fee_token_addresses.\
         eth_fee_token_address",
        "batcher_config.static_config.block_builder_config.chain_info.fee_token_addresses.\
         strk_fee_token_address",
        "batcher_config.static_config.block_builder_config.execute_config.n_workers",
        "batcher_config.static_config.block_builder_config.versioned_constants_overrides",
        "batcher_config.static_config.contract_class_manager_config.native_compiler_config.\
         max_cpu_time",
        "batcher_config.static_config.first_block_with_partial_block_hash",
        "batcher_config.static_config.pre_confirmed_cende_config.recorder_url",
        "batcher_config.static_config.storage.db_config.chain_id",
        "batcher_config.static_config.validation_only",
        "class_manager_config.static_config.class_storage_config.class_hash_storage_config.\
         db_config.chain_id",
        "class_manager_config.static_config.class_manager_config.\
         max_compiled_contract_class_object_size",
        "committer_config.storage_config.cache_size",
        "committer_config.verify_state_diff_hash",
        "consensus_manager_config.cende_config.recorder_url",
        "consensus_manager_config.consensus_manager_config.dynamic_config.\
         require_virtual_proposer_vote",
        "consensus_manager_config.consensus_manager_config.dynamic_config.timeouts.proposal.base",
        "consensus_manager_config.consensus_manager_config.dynamic_config.timeouts.proposal.max",
        "consensus_manager_config.consensus_manager_config.dynamic_config.validator_id",
        "consensus_manager_config.consensus_manager_config.static_config.storage_config.db_config.\
         chain_id",
        "consensus_manager_config.context_config.dynamic_config.build_proposal_margin_millis",
        "consensus_manager_config.context_config.dynamic_config.compare_retrospective_block_hash",
        "consensus_manager_config.context_config.dynamic_config.min_l2_gas_price_per_height",
        "consensus_manager_config.context_config.dynamic_config.override_eth_to_fri_rate",
        "consensus_manager_config.context_config.dynamic_config.override_l1_data_gas_price_fri",
        "consensus_manager_config.context_config.dynamic_config.override_l1_gas_price_fri",
        "consensus_manager_config.context_config.dynamic_config.override_l2_gas_price_fri",
        "consensus_manager_config.context_config.static_config.behavior_mode",
        "consensus_manager_config.context_config.static_config.chain_id",
        "consensus_manager_config.network_config.advertised_multiaddr",
        "consensus_manager_config.network_config.bootstrap_peer_multiaddr",
        "consensus_manager_config.network_config.chain_id",
        "consensus_manager_config.network_config.port",
        "consensus_manager_config.revert_config.revert_up_to_and_including",
        "consensus_manager_config.revert_config.should_revert",
        "consensus_manager_config.staking_manager_config.dynamic_config.default_committee",
        "consensus_manager_config.staking_manager_config.dynamic_config.override_committee",
        "gateway_config.dynamic_config.native_classes_whitelist",
        "gateway_config.static_config.authorized_declarer_accounts",
        "gateway_config.static_config.chain_info.chain_id",
        "gateway_config.static_config.chain_info.fee_token_addresses.eth_fee_token_address",
        "gateway_config.static_config.chain_info.fee_token_addresses.strk_fee_token_address",
        "gateway_config.static_config.contract_class_manager_config.native_compiler_config.\
         max_cpu_time",
        "gateway_config.static_config.proof_archive_writer_config.bucket_name",
        "gateway_config.static_config.stateful_tx_validator_config.max_allowed_nonce_gap",
        "gateway_config.static_config.stateful_tx_validator_config.validate_resource_bounds",
        "gateway_config.static_config.stateful_tx_validator_config.versioned_constants_overrides",
        "gateway_config.static_config.stateless_tx_validator_config.max_contract_bytecode_size",
        "gateway_config.static_config.stateless_tx_validator_config.min_gas_price",
        "gateway_config.static_config.stateless_tx_validator_config.validate_resource_bounds",
        "http_server_config.static_config.port",
        "l1_events_scraper_config.chain_id",
        "l1_gas_price_scraper_config.chain_id",
        "mempool_config.dynamic_config.transaction_ttl",
        "mempool_config.static_config.behavior_mode",
        "mempool_config.static_config.recorder_url",
        "mempool_config.static_config.validate_resource_bounds",
        "mempool_p2p_config.network_config.advertised_multiaddr",
        "mempool_p2p_config.network_config.bootstrap_peer_multiaddr",
        "mempool_p2p_config.network_config.chain_id",
        "mempool_p2p_config.network_config.port",
        "monitoring_endpoint_config.port",
        "sierra_compiler_config.audited_libfuncs_only",
        "sierra_compiler_config.max_bytecode_size",
        "sierra_compiler_config.max_cpu_time",
        "state_sync_config.static_config.central_sync_client_config",
        "state_sync_config.static_config.network_config",
        "state_sync_config.static_config.p2p_sync_client_config",
        "state_sync_config.static_config.revert_config.revert_up_to_and_including",
        "state_sync_config.static_config.revert_config.should_revert",
        "state_sync_config.static_config.rpc_config.chain_id",
        "state_sync_config.static_config.rpc_config.execution_config.eth_fee_contract_address",
        "state_sync_config.static_config.rpc_config.execution_config.strk_fee_contract_address",
        "state_sync_config.static_config.rpc_config.port",
        "state_sync_config.static_config.rpc_config.starknet_url",
        "state_sync_config.static_config.storage_config.db_config.chain_id",
    ])
});

const COMPONENTS_LIBSONNET_PATH: &str =
    "crates/apollo_deployments/jsonnet/lib/components.libsonnet";
const MUST_OVERRIDE_EXPR: &str = "error \"must be set by node config\"";
const MUST_OVERRIDE_SENTINEL: &str = "\"__MUST_OVERRIDE__\"";

/// Verifies that every path in `KEYS_TO_BE_REPLACED` exists in `components.libsonnet` and has
/// `mustOverride` (the error expression) as its value.
pub fn test_components_libsonnet_is_valid() {
    let source = fs::read_to_string(COMPONENTS_LIBSONNET_PATH)
        .unwrap_or_else(|e| panic!("Failed to read {COMPONENTS_LIBSONNET_PATH}: {e}"));

    // Substitute the error expression with a sentinel string so the file evaluates cleanly.
    let testable = source.replace(MUST_OVERRIDE_EXPR, MUST_OVERRIDE_SENTINEL);

    let val = State::default()
        .evaluate_snippet("components.libsonnet", testable)
        .expect("components.libsonnet failed to evaluate after sentinel substitution");

    let components: Value =
        serde_json::to_value(&val).expect("components.libsonnet result is not serializable");

    let mut missing: Vec<&str> = KEYS_TO_BE_REPLACED
        .iter()
        .copied()
        .filter(|path| !path_has_sentinel(&components, path))
        .collect();

    if !missing.is_empty() {
        missing.sort_unstable();
        panic!(
            "The following paths are missing from components.libsonnet or do not use \
             `mustOverride`:\n{}",
            missing.join("\n")
        );
    }
}

/// Returns true if any top-level component object contains `path` with the sentinel value.
fn path_has_sentinel(components: &Value, path: &str) -> bool {
    let Value::Object(top) = components else { return false };
    top.values()
        .any(|component| {
            get_nested(component, path).is_some_and(|v| v.as_str() == Some("__MUST_OVERRIDE__"))
        })
}

fn get_nested<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let (head, tail) = match path.split_once('.') {
        Some((h, t)) => (h, Some(t)),
        None => (path, None),
    };
    let next = value.as_object()?.get(head)?;
    match tail {
        Some(rest) => get_nested(next, rest),
        None => Some(next),
    }
}
