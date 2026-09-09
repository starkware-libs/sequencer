// The universal component catalog: the split of nodes's components into reactive and active
// components.
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
