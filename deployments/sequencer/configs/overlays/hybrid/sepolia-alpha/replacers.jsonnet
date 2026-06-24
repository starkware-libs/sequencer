// replacers for the `sepolia-alpha` environment (hybrid layout): only the values that differ from
// the applicative-config defaults (a flat dotted-key map). Everything else uses the schema default.
{
  'batcher_config.dynamic_config.n_concurrent_txs': 8,
  'batcher_config.static_config.block_builder_config.bouncer_config.block_max_capacity.state_diff_size': 5000,
  'committer_config.storage_config.inner_storage_config.cache_size': 1073741824,
}
