// replacers for the `testing/node-0` overlay (hybrid layout): only the values that differ from the
// applicative-config defaults (a flat dotted-key map). Everything else uses the schema default.
{
  eth_fee_token_address: '0x1001',
  strk_fee_token_address: '0x1002',
  'batcher_config.dynamic_config.proposer_idle_detection_delay_millis': 2000,
  'batcher_config.static_config.block_builder_config.execute_config.n_workers': 28,
  'committer_config.storage_config.cache_size': 1000000,
  'consensus_manager_config.consensus_manager_config.dynamic_config.timeouts.proposal.max': 15.0,
  'consensus_manager_config.context_config.dynamic_config.compare_retrospective_block_hash': false,
  'gateway_config.static_config.stateless_tx_validator_config.min_gas_price': 3000000000,
  'sierra_compiler_config.audited_libfuncs_only': false,
  'state_sync_config.static_config.central_sync_client_config': null,
  'state_sync_config.static_config.network_config': {
    port: 55010,
  },
}
