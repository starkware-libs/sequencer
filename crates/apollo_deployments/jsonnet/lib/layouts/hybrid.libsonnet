// Hybrid layout (6 services): pure data consumed by lib/infra/derive.libsonnet.
// Transcribed from crates/apollo_deployments/src/deployments/hybrid.rs (per-service component
// assignments + scale/retries policy) and the deployed overlays/hybrid/common/services/*.yaml
// (component ports + service DNS names). See derive.libsonnet for the field contract.
local catalog = import '../infra/catalog.libsonnet';
local t = import '../infra/templates.libsonnet';
{
  reactive:: catalog.reactive,
  active:: catalog.active,
  // Hosted-but-not-remote-served reactive components (LocalExecutionWithRemoteDisabled when hosted).
  localOnly:: ['config_manager', 'mempool_p2p'],

  ports:: {
    batcher: 55000,
    class_manager: 55001,
    gateway: 55002,
    l1_gas_price_provider: 55003,
    l1_events_provider: 55004,
    mempool: 55006,
    sierra_compiler: 55007,
    signature_manager: 55008,
    state_sync: 55009,
    proof_manager: 55012,
    committer: 55013,
  },
  serviceDns:: {
    committer: 'sequencer-committer-service',
    core: 'sequencer-core-service',
    gateway: 'sequencer-gateway-service',
    l1: 'sequencer-l1-service',
    mempool: 'sequencer-mempool-service',
    sierra_compiler: 'sequencer-sierracompiler-service',
  },
  // Home service of each remote-served reactive component (for url/port resolution).
  homeOf:: {
    batcher: 'core',
    class_manager: 'core',
    proof_manager: 'core',
    signature_manager: 'core',
    state_sync: 'core',
    committer: 'committer',
    gateway: 'gateway',
    l1_events_provider: 'l1',
    l1_gas_price_provider: 'l1',
    mempool: 'mempool',
    sierra_compiler: 'sierra_compiler',
  },
  // Remote-client tuning of components homed in each service (scale_policy + retries from hybrid.rs).
  scale:: {
    committer: { idle: t.IDLE_STATIC, retries: t.RETRIES_DEFAULT },
    core: { idle: t.IDLE_STATIC, retries: t.RETRIES_DEFAULT },
    gateway: { idle: t.IDLE_AUTO_SCALED, retries: t.RETRIES_DEFAULT },
    l1: { idle: t.IDLE_STATIC, retries: t.RETRIES_L1 },
    mempool: { idle: t.IDLE_STATIC, retries: t.RETRIES_DEFAULT },
    sierra_compiler: { idle: t.IDLE_AUTO_SCALED, retries: t.RETRIES_DEFAULT },
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
