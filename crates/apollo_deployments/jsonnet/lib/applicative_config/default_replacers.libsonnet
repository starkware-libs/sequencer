// Default replacers: the fallback value for every overridable applicative-config key, as a flat
// dotted-key map.
local defaults = import '../defaults.libsonnet';

{
  eth_fee_token_address: defaults.ETH_FEE_TOKEN_ADDRESS,
  strk_fee_token_address: defaults.STRK_FEE_TOKEN_ADDRESS,
  // Consumed by the batcher and gateway applicative configs; default is the empty whitelist.
  native_classes_whitelist: '[]',
  versioned_constants_overrides: null,
  // Consumed by build.libsonnet (top-level validation_only) and the batcher applicative config.
  validation_only: defaults.VALIDATION_ONLY,

  'batcher_config.dynamic_config.n_concurrent_txs': 100,
  'batcher_config.dynamic_config.proposer_idle_detection_delay_millis': 1500,
  'batcher_config.static_config.block_builder_config.bouncer_config.block_max_capacity.n_events': 5000,
  'batcher_config.static_config.block_builder_config.bouncer_config.block_max_capacity.receipt_l2_gas': 5800000000,
  'batcher_config.static_config.block_builder_config.bouncer_config.block_max_capacity.state_diff_size': 4000,
  'batcher_config.static_config.block_builder_config.execute_config.n_workers': 5,
  'batcher_config.static_config.first_block_with_partial_block_hash': null,

  'committer_config.storage_config.cache_size': 10000000,
  'committer_config.storage_config.inner_storage_config.cache_size': 8589934592,

  'consensus_manager_config.consensus_manager_config.dynamic_config.timeouts.proposal.base': 9.1,
  'consensus_manager_config.consensus_manager_config.dynamic_config.timeouts.proposal.max': 9.1,
  'consensus_manager_config.context_config.dynamic_config.min_l2_gas_price_per_height': '',
  'consensus_manager_config.context_config.dynamic_config.override_eth_to_fri_rate': null,
  'consensus_manager_config.context_config.dynamic_config.override_l1_data_gas_price_fri': null,
  'consensus_manager_config.context_config.dynamic_config.override_l1_gas_price_fri': null,
  'consensus_manager_config.context_config.dynamic_config.override_l2_gas_price_fri': null,

  'gateway_config.static_config.authorized_declarer_accounts': null,
  'gateway_config.static_config.stateful_tx_validator_config.max_allowed_nonce_gap': 200,
  'gateway_config.static_config.stateless_tx_validator_config.max_contract_bytecode_size': 81920,
  'gateway_config.static_config.stateless_tx_validator_config.min_gas_price': 8000000000,

  'mempool_config.dynamic_config.transaction_ttl': 300,

  'sierra_compiler_config.audited_libfuncs_only': true,
  'sierra_compiler_config.max_bytecode_size': 81920,

  // Enable the central sync client by default.
  'state_sync_config.static_config.central_sync_client_config': {},
  'state_sync_config.static_config.network_config': null,
  'state_sync_config.static_config.p2p_sync_client_config': null,
}
