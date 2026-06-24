// replacers for the `mainnet` environment (hybrid layout): only the values that differ from the
// applicative-config defaults (a flat dotted-key map). Everything else uses the schema default.
{
  'batcher_config.static_config.block_builder_config.bouncer_config.block_max_capacity.state_diff_size': 5000,
  'batcher_config.static_config.block_builder_config.execute_config.n_workers': 12,
  'committer_config.storage_config.cache_size': 50000000,
  'consensus_manager_config.context_config.dynamic_config.min_l2_gas_price_per_height': '8269292:27400000000,8742344:30100000000',
  'state_sync_config.static_config.central_sync_client_config': {
    sync_config: {
      store_sierras_and_casms_block_threshold: 103129,
    },
  },
  'state_sync_config.static_config.network_config': {
    port: 55010,
  },
}
