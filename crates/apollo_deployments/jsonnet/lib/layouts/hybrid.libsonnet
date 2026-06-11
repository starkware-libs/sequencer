// Hybrid layout structure.
local catalog = import '../infra/catalog.libsonnet';
local templates = import '../infra/templates.libsonnet';
{
  reactive:: catalog.reactive,
  active:: catalog.active,
  // Hosted-but-not-remote-served reactive components.
  localOnly:: ['config_manager', 'mempool_p2p'],

  // Canonical per-component infra ports (shared across layouts).
  ports:: templates.componentPorts,
  serviceDns:: {
    committer: 'sequencer-committer-service',
    core: 'sequencer-core-service',
    gateway: 'sequencer-gateway-service',
    l1: 'sequencer-l1-service',
    mempool: 'sequencer-mempool-service',
    sierra_compiler: 'sequencer-sierracompiler-service',
  },
  // Remote-client tuning of components homed in each service (scale_policy + retries from hybrid.rs).
  scale:: {
    committer: { idle: templates.IDLE_STATIC, retries: templates.RETRIES_DEFAULT },
    core: { idle: templates.IDLE_STATIC, retries: templates.RETRIES_DEFAULT },
    gateway: { idle: templates.IDLE_AUTO_SCALED, retries: templates.RETRIES_DEFAULT },
    l1: { idle: templates.IDLE_STATIC, retries: templates.RETRIES_L1 },
    mempool: { idle: templates.IDLE_STATIC, retries: templates.RETRIES_DEFAULT },
    sierra_compiler: { idle: templates.IDLE_AUTO_SCALED, retries: templates.RETRIES_DEFAULT },
  },
  // Per service: components hosted (runs) and reactive components consumed remotely (uses).
  // Everything not listed is Disabled. Transcribed from hybrid.rs get_*_component_config.
  services:: {
    committer: {
      runs: ['committer', 'config_manager', 'monitoring_endpoint'],
      uses: ['batcher'],
    },
    core: {
      runs: [
        'batcher',
        'class_manager',
        'proof_manager',
        'signature_manager',
        'state_sync',
        'config_manager',
        'consensus_manager',
        'monitoring_endpoint',
      ],
      uses: ['committer', 'l1_events_provider', 'l1_gas_price_provider', 'mempool', 'sierra_compiler'],
    },
    gateway: {
      runs: ['gateway', 'config_manager', 'http_server', 'monitoring_endpoint'],
      uses: ['class_manager', 'mempool', 'proof_manager', 'state_sync'],
    },
    l1: {
      runs: [
        'l1_events_provider',
        'l1_gas_price_provider',
        'config_manager',
        'l1_events_scraper',
        'l1_gas_price_scraper',
        'monitoring_endpoint',
      ],
      uses: ['batcher', 'state_sync'],
    },
    mempool: {
      runs: ['mempool', 'mempool_p2p', 'config_manager', 'monitoring_endpoint'],
      uses: ['class_manager', 'gateway', 'proof_manager'],
    },
    sierra_compiler: {
      runs: ['sierra_compiler', 'config_manager', 'monitoring_endpoint'],
      uses: [],
    },
  },
}
