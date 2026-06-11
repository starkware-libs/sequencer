// Fixed infra building blocks shared by every layout: the server/client config templates and the
// scalar constants used when classifying components.
{
  DISABLED_PORT:: 0,
  LOCALHOST:: 'localhost',

  // Remote-client tuning, selected per home service.
  IDLE_STATIC:: 10,  // ScalePolicy::StaticallyScaled
  IDLE_AUTO_SCALED:: 0,  // ScalePolicy::AutoScaled
  RETRIES_DEFAULT:: 15,  // apollo_infra DEFAULT_RETRIES
  RETRIES_L1:: 0,  // RETRIES_FOR_L1_SERVICES

  // Canonical infra port of each remote-served reactive component, shared by every layout.
  // Transcribed from the deployed hybrid overlays (overlays/hybrid/common/services/*.yaml,
  // e.g. core.yaml's `components.<c>.port`). A component's infra port is the same wherever it runs.
  componentPorts:: {
    batcher: 55000,
    class_manager: 55001,
    committer: 55013,
    gateway: 55002,
    l1_events_provider: 55004,
    l1_gas_price_provider: 55003,
    mempool: 55006,
    proof_manager: 55012,
    sierra_compiler: 55007,
    signature_manager: 55008,
    state_sync: 55009,
  },

  // LocalServerConfig::default().
  localServerConfig:: {
    high_priority_requests_channel_capacity: 1024,
    inbound_requests_channel_capacity: 1024,
    max_concurrency: 128,
    normal_priority_requests_channel_capacity: 1024,
    processing_time_warning_threshold_ms: 3000,
  },

  // RemoteServerConfig::default().
  remoteServerConfig:: {
    bind_ip: '0.0.0.0',
    keepalive_interval_ms: 30000,
    keepalive_timeout_ms: 10000,
    max_concurrency: 128,
    max_request_body_bytes: 8388608,
    max_streams_per_connection: 8,
    set_tcp_nodelay: true,
  },

  // RemoteClientConfig::default(), with `retries`/`idle_connections` set per the home service's
  // scale policy (every other field is the apollo_infra default).
  remoteClientConfig(retries, idleConnections):: {
    attempts_per_log: 1,
    connection_timeout_ms: 500,
    idle_connections: idleConnections,
    initial_retry_delay_ms: 1,
    keepalive_timeout_ms: 30000,
    max_response_body_bytes: 8388608,
    max_retry_interval_ms: 1000,
    request_timeout_ms: 30000,
    retries: retries,
    set_tcp_nodelay: true,
  },
}
