local defaults = import '../defaults.libsonnet';

{
  eth_fee_token_address: defaults.ETH_FEE_TOKEN_ADDRESS,
  strk_fee_token_address: defaults.STRK_FEE_TOKEN_ADDRESS,
  native_classes_whitelist: '[]',
  versioned_constants_overrides: null,
  validation_only: defaults.VALIDATION_ONLY,

  n_concurrent_txs: 100,
  proposer_idle_detection_delay_millis: 1500,
  max_events_in_block: 5000,
  max_receipt_l2_gas_in_block: 5800000000,
  max_state_diff_in_block: 4000,
  n_execution_workers: 5,
  first_block_with_partial_block_hash: null,

  committer_cache_size: 10000000,
  committer_inner_storage_cache_size: 8589934592,

  proposal_timeout_base: 9.1,
  proposal_timeout_max: 9.1,
  min_l2_gas_price_per_height: '',
  override_eth_to_fri_rate: null,
  override_l1_data_gas_price_fri: null,
  override_l1_gas_price_fri: null,
  override_l2_gas_price_fri: null,
  compare_retrospective_block_hash: true,

  authorized_declarer_accounts: null,
  max_allowed_nonce_gap: 200,
  max_contract_bytecode_size: 81920,
  min_gas_price: 8000000000,

  transaction_ttl: 300,

  audited_libfuncs_only: true,
  max_bytecode_size: 81920,

  // Enable the central sync client by default.
  central_sync_client_config: {},
  state_sync_network_config: null,
  p2p_sync_client_config: null,
}
