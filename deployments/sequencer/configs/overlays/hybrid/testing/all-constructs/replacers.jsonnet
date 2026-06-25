// replacers for the `testing/all-constructs` overlay (hybrid layout): only the values that differ
// from the applicative-config defaults (a flat dotted-key map). Everything else uses the schema
// default. Dummy values; nothing here is deployed.
{
  'batcher_config.dynamic_config.n_concurrent_txs': 1,
  'batcher_config.static_config.block_builder_config.execute_config.n_workers': 1,
  'committer_config.storage_config.cache_size': 1000000,
  'sierra_compiler_config.audited_libfuncs_only': false,
  'state_sync_config.static_config.central_sync_client_config': null,
}
