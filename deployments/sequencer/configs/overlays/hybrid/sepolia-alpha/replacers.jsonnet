// replacers for the `sepolia-alpha` environment (hybrid layout): only the values that differ from
// the applicative-config defaults (a flat dotted-key map). Everything else uses the schema default.
{
  native_classes_whitelist: 'All',
  'batcher_config.dynamic_config.n_concurrent_txs': 8,
  'batcher_config.static_config.block_builder_config.bouncer_config.block_max_capacity.state_diff_size': 5000,
  'batcher_config.static_config.first_block_with_partial_block_hash': {
    block_hash: '0x578b4e2f34e4da24e7482de643b4e3435fa7e34770cdb8d71002bb19e415ffa',
    block_number: 86311,
    parent_block_hash: '0x5c980ea7747167d2ae98fa7ef7d62f52243e924c453b4934045443d977458d3',
  },
  'committer_config.storage_config.inner_storage_config.cache_size': 1073741824,
}
