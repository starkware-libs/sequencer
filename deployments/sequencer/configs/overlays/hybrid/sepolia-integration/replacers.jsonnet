// replacers for the `sepolia-integration` environment (hybrid layout): only the values that differ
// from the applicative-config defaults (a flat dotted-key map). Everything else uses the schema
// default.
{
  native_classes_whitelist: 'All',
  'batcher_config.dynamic_config.n_concurrent_txs': 2,
  'batcher_config.static_config.block_builder_config.execute_config.n_workers': 1,
  'batcher_config.static_config.first_block_with_partial_block_hash': {
    block_hash: '0x1ea2a9cfa3df5297d58c0a04d09d276bc68d40fe64701305bbe2ed8f417e869',
    block_number: 35748,
    parent_block_hash: '0x77140bef51bbb4d1932f17cc5081825ff18465a1df4440ca0429a4fa80f1dc5',
  },
  'sierra_compiler_config.audited_libfuncs_only': false,
}
