// Shared helpers for defining service topology configurations.
// These functions produce nested JSON objects that deep-merge into the service config via +:.

local RETRIES = 150;
local L1_RETRIES = 0;
local IDLE_STATIC = 10;
local IDLE_AUTO_SCALED = 0;
local INFRA_PORT = 1;    // Placeholder replaced by infra at deploy time.
local DISABLED_PORT = 0;

local localServerConfig = {
  high_priority_requests_channel_capacity: 1024,
  inbound_requests_channel_capacity: 1024,
  max_concurrency: 128,
  normal_priority_requests_channel_capacity: 1024,
  processing_time_warning_threshold_ms: 3000,
};

local remoteServerConfig = {
  bind_ip: "0.0.0.0",
  keepalive_interval_ms: 30000,
  keepalive_timeout_ms: 10000,
  max_concurrency: 128,
  max_request_body_bytes: 8388608,
  max_streams_per_connection: 8,
  set_tcp_nodelay: true,
};

local remoteClientConfig(retries, idleConnections) = {
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
};

{
  // Component runs locally and accepts inbound connections from remote components.
  localComponent(name):
    { components +: { [name] +: {
      execution_mode: "LocalExecutionWithRemoteEnabled",
      local_server_config: localServerConfig,
      port: INFRA_PORT,
      remote_client_config: null,
      remote_server_config: remoteServerConfig,
      url: "remote_service",
    } } },

  // Component is accessed via a remote call (runs in another service).
  remoteComponent(name, retries=RETRIES, idleConnections=IDLE_STATIC):
    { components +: { [name] +: {
      execution_mode: "Remote",
      local_server_config: null,
      port: INFRA_PORT,
      remote_client_config: remoteClientConfig(retries, idleConnections),
      remote_server_config: null,
      url: "remote_service",
    } } },

  // Component runs locally but is not accessible from remote services.
  localOnlyComponent(name):
    { components +: { [name] +: {
      execution_mode: "LocalExecutionWithRemoteDisabled",
      local_server_config: localServerConfig,
      port: DISABLED_PORT,
      remote_client_config: null,
      remote_server_config: null,
      url: "localhost",
    } } },

  // Component is fully disabled for this service.
  disabledComponent(name):
    { components +: { [name] +: {
      execution_mode: "Disabled",
      local_server_config: null,
      port: DISABLED_PORT,
      remote_client_config: null,
      remote_server_config: null,
      url: "localhost",
    } } },

  // Component is enabled with no server config (e.g., consensus_manager, monitoring_endpoint).
  enabledComponent(name): {
    components +: { [name] +: { execution_mode: "Enabled" } },
  },

  // Component with no server config that is disabled for this service.
  disabledSimpleComponent(name): {
    components +: { [name] +: { execution_mode: "Disabled" } },
  },

  // Constants exposed for caller use.
  RETRIES: RETRIES,
  L1_RETRIES: L1_RETRIES,
  IDLE_STATIC: IDLE_STATIC,
  IDLE_AUTO_SCALED: IDLE_AUTO_SCALED,
  INFRA_PORT: INFRA_PORT,
  DISABLED_PORT: DISABLED_PORT,
}
