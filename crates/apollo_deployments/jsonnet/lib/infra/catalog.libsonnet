// The universal component catalog: the split of `SequencerNodeConfig`'s components into reactive
// (full execution config) and active (execution_mode only). This is a property of the Rust config
// (crates/apollo_node_config/src/component_config.rs), independent of deployment layout — every
// layout descriptor imports these lists rather than restating them.
{
  reactive:: [
    'batcher',
    'class_manager',
    'committer',
    'config_manager',
    'gateway',
    'l1_events_provider',
    'l1_gas_price_provider',
    'mempool',
    'mempool_p2p',
    'proof_manager',
    'sierra_compiler',
    'signature_manager',
    'state_sync',
  ],
  active:: [
    'consensus_manager',
    'http_server',
    'l1_events_scraper',
    'l1_gas_price_scraper',
    'monitoring_endpoint',
  ],
}
