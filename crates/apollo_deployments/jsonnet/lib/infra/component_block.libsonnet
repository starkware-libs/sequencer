// Builders for a single component's infra config entry. Each takes already-resolved values and emits
// the exact field shape of the Rust ReactiveComponentExecutionConfig / ActiveComponentExecutionConfig
// (crates/apollo_node_config/src/component_execution_config.rs) — `null` for absent optional
// sub-configs, so the optional-presence invariants validated there hold by construction.
local t = import 'templates.libsonnet';
{
  // Reactive, hosted locally and reachable remotely (real port + home-service url).
  localRemoteEnabled(url, port):: {
    execution_mode: 'LocalExecutionWithRemoteEnabled',
    local_server_config: t.localServerConfig,
    remote_server_config: t.remoteServerConfig,
    remote_client_config: null,
    url: url,
    port: port,
  },

  // Reactive, hosted locally but not remote-served (e.g. config_manager, mempool_p2p; all of
  // consolidated). No port/url needed.
  localRemoteDisabled():: {
    execution_mode: 'LocalExecutionWithRemoteDisabled',
    local_server_config: t.localServerConfig,
    remote_server_config: null,
    remote_client_config: null,
    url: t.LOCALHOST,
    port: t.DISABLED_PORT,
  },

  // Reactive, consumed from another service. retries/idle reflect the HOME service's scale policy.
  remote(url, port, retries, idleConnections):: {
    execution_mode: 'Remote',
    local_server_config: null,
    remote_server_config: null,
    remote_client_config: t.remoteClientConfig(retries, idleConnections),
    url: url,
    port: port,
  },

  disabledReactive():: {
    execution_mode: 'Disabled',
    local_server_config: null,
    remote_server_config: null,
    remote_client_config: null,
    url: t.LOCALHOST,
    port: t.DISABLED_PORT,
  },

  // Active components carry only an execution_mode.
  activeEnabled():: { execution_mode: 'Enabled' },
  activeDisabled():: { execution_mode: 'Disabled' },
}
