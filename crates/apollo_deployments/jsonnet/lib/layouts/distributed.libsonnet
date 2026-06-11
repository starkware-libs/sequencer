// Distributed layout structure.
local catalog = import '../infra/catalog.libsonnet';
local templates = import '../infra/templates.libsonnet';
{
  reactive:: catalog.reactive,
  active:: catalog.active,
  // Hosted-but-not-remote-served reactive components (LocalExecutionWithRemoteDisabled when hosted).
  localOnly:: ['config_manager', 'mempool_p2p'],

  // Canonical per-component infra ports (shared across layouts).
  ports:: templates.componentPorts,
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
  // Remote-client tuning of components homed in each service (get_scale_policy + get_retries).
  // Gateway/SierraCompiler are AutoScaled (idle 0); L1 uses RETRIES_FOR_L1_SERVICES (0); rest static.
  scale:: {
    batcher: { idle: templates.IDLE_STATIC, retries: templates.RETRIES_DEFAULT },
    class_manager: { idle: templates.IDLE_STATIC, retries: templates.RETRIES_DEFAULT },
    committer: { idle: templates.IDLE_STATIC, retries: templates.RETRIES_DEFAULT },
    gateway: { idle: templates.IDLE_AUTO_SCALED, retries: templates.RETRIES_DEFAULT },
    l1: { idle: templates.IDLE_STATIC, retries: templates.RETRIES_L1 },
    mempool: { idle: templates.IDLE_STATIC, retries: templates.RETRIES_DEFAULT },
    proof_manager: { idle: templates.IDLE_STATIC, retries: templates.RETRIES_DEFAULT },
    sierra_compiler: { idle: templates.IDLE_AUTO_SCALED, retries: templates.RETRIES_DEFAULT },
    signature_manager: { idle: templates.IDLE_STATIC, retries: templates.RETRIES_DEFAULT },
    state_sync: { idle: templates.IDLE_STATIC, retries: templates.RETRIES_DEFAULT },
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
