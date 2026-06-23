// replacers for the `sepolia-integration` environment (hybrid layout): only the values that differ
// from the applicative-config defaults (a flat dotted-key map). Everything else uses the schema
// default.
{
  'batcher_config.dynamic_config.n_concurrent_txs': 2,
  'batcher_config.static_config.block_builder_config.execute_config.n_workers': 1,
  'sierra_compiler_config.audited_libfuncs_only': false,
}
