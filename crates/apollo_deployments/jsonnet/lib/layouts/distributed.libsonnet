// Distributed layout (12 services, one per component domain): pure data consumed by
// lib/infra/derive.libsonnet. Transcribed from crates/apollo_deployments/src/deployments/distributed.rs
// (per-service get_*_component_config + get_scale_policy/get_retries). `config_manager` and
// `mempool_p2p` are hosted local-only wherever they run; `monitoring_endpoint` is active in every
// service. url/port (serviceDns/ports) are deploy-time values not asserted by the infra-parity test
// (the Rust side leaves them as placeholders); they follow the sequencer-<service>-service / infra
// port-allocation convention.
local catalog = import '../infra/catalog.libsonnet';
local t = import '../infra/templates.libsonnet';
{
  reactive:: catalog.reactive,
  active:: catalog.active,
  // Hosted-but-not-remote-served reactive components (LocalExecutionWithRemoteDisabled when hosted).
  localOnly:: ['config_manager', 'mempool_p2p'],

  // Port of each remote-served reactive component, in distributed.rs infra_port_allocator order.
  ports:: {
    batcher: 55000,
    class_manager: 55001,
    committer: 55002,
    gateway: 55003,
    l1_gas_price_provider: 55004,
    l1_events_provider: 55005,
    mempool: 55006,
    proof_manager: 55007,
    sierra_compiler: 55008,
    signature_manager: 55009,
    state_sync: 55010,
  },
  serviceDns:: {
    batcher: 'sequencer-batcher-service',
    class_manager: 'sequencer-class-manager-service',
    committer: 'sequencer-committer-service',
    gateway: 'sequencer-gateway-service',
    l1: 'sequencer-l1-service',
    mempool: 'sequencer-mempool-service',
    proof_manager: 'sequencer-proof-manager-service',
    sierra_compiler: 'sequencer-sierra-compiler-service',
    signature_manager: 'sequencer-signature-manager-service',
    state_sync: 'sequencer-state-sync-service',
  },
  // Home service of each remote-served reactive component. Both l1 providers home to the `l1` service.
  homeOf:: {
    batcher: 'batcher',
    class_manager: 'class_manager',
    committer: 'committer',
    gateway: 'gateway',
    l1_gas_price_provider: 'l1',
    l1_events_provider: 'l1',
    mempool: 'mempool',
    proof_manager: 'proof_manager',
    sierra_compiler: 'sierra_compiler',
    signature_manager: 'signature_manager',
    state_sync: 'state_sync',
  },
  // Remote-client tuning of components homed in each service (get_scale_policy + get_retries).
  // Gateway/SierraCompiler are AutoScaled (idle 0); L1 uses RETRIES_FOR_L1_SERVICES (0); rest static.
  scale:: {
    batcher: { idle: t.IDLE_STATIC, retries: t.RETRIES_DEFAULT },
    class_manager: { idle: t.IDLE_STATIC, retries: t.RETRIES_DEFAULT },
    committer: { idle: t.IDLE_STATIC, retries: t.RETRIES_DEFAULT },
    gateway: { idle: t.IDLE_AUTO_SCALED, retries: t.RETRIES_DEFAULT },
    l1: { idle: t.IDLE_STATIC, retries: t.RETRIES_L1 },
    mempool: { idle: t.IDLE_STATIC, retries: t.RETRIES_DEFAULT },
    proof_manager: { idle: t.IDLE_STATIC, retries: t.RETRIES_DEFAULT },
    sierra_compiler: { idle: t.IDLE_AUTO_SCALED, retries: t.RETRIES_DEFAULT },
    signature_manager: { idle: t.IDLE_STATIC, retries: t.RETRIES_DEFAULT },
    state_sync: { idle: t.IDLE_STATIC, retries: t.RETRIES_DEFAULT },
  },
  // Per service: components hosted (runs) and reactive components consumed remotely (uses).
  // Everything not listed is Disabled. Transcribed from distributed.rs get_*_component_config.
  services:: {
    batcher: {
      runs: ['batcher', 'config_manager', 'monitoring_endpoint'],
      uses: ['class_manager', 'committer', 'l1_events_provider', 'mempool', 'proof_manager'],
    },
    class_manager: {
      runs: ['class_manager', 'config_manager', 'monitoring_endpoint'],
      uses: ['sierra_compiler'],
    },
    committer: {
      runs: ['committer', 'config_manager', 'monitoring_endpoint'],
      uses: ['batcher'],
    },
    consensus_manager: {
      runs: ['config_manager', 'consensus_manager', 'monitoring_endpoint'],
      uses: ['batcher', 'class_manager', 'l1_gas_price_provider', 'proof_manager', 'state_sync', 'signature_manager'],
    },
    http_server: {
      runs: ['config_manager', 'http_server', 'monitoring_endpoint'],
      uses: ['gateway'],
    },
    gateway: {
      runs: ['gateway', 'config_manager', 'monitoring_endpoint'],
      uses: ['class_manager', 'mempool', 'proof_manager', 'state_sync'],
    },
    l1: {
      runs: [
        'l1_gas_price_provider',
        'l1_events_provider',
        'config_manager',
        'l1_gas_price_scraper',
        'l1_events_scraper',
        'monitoring_endpoint',
      ],
      uses: ['state_sync', 'batcher'],
    },
    proof_manager: {
      runs: ['proof_manager', 'config_manager', 'monitoring_endpoint'],
      uses: [],
    },
    mempool: {
      runs: ['mempool', 'mempool_p2p', 'config_manager', 'monitoring_endpoint'],
      uses: ['class_manager', 'gateway', 'proof_manager'],
    },
    sierra_compiler: {
      runs: ['sierra_compiler', 'config_manager', 'monitoring_endpoint'],
      uses: [],
    },
    signature_manager: {
      runs: ['signature_manager', 'config_manager', 'monitoring_endpoint'],
      uses: [],
    },
    state_sync: {
      runs: ['state_sync', 'config_manager', 'monitoring_endpoint'],
      uses: ['class_manager'],
    },
  },
}
