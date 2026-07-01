// Builders for a single component's infra config entry.
local templates = import 'templates.libsonnet';
{
  // Reactive, hosted locally and reachable remotely (real port + home-service url).
  localRemoteEnabled(url, port):: {
    execution_mode: 'LocalExecutionWithRemoteEnabled',
    local_server_config: templates.localServerConfig,
    remote_server_config: templates.remoteServerConfig,
    remote_client_config: null,
    url: url,
    port: port,
  },

  // Reactive, hosted locally but not remote-served, no port/url needed.
  localRemoteDisabled():: {
    execution_mode: 'LocalExecutionWithRemoteDisabled',
    local_server_config: templates.localServerConfig,
    remote_server_config: null,
    remote_client_config: null,
    url: templates.LOCALHOST,
    port: templates.DISABLED_PORT,
  },

  // Reactive, consumed from another service. retries/idle reflect the HOME service's scale policy.
  remote(url, port, retries, idleConnections):: {
    execution_mode: 'Remote',
    local_server_config: null,
    remote_server_config: null,
    remote_client_config: templates.remoteClientConfig(retries, idleConnections),
    url: url,
    port: port,
  },

  disabledReactive():: {
    execution_mode: 'Disabled',
    local_server_config: null,
    remote_server_config: null,
    remote_client_config: null,
    url: templates.LOCALHOST,
    port: templates.DISABLED_PORT,
  },

  // Active components carry only an execution_mode.
  activeEnabled():: { execution_mode: 'Enabled' },
  activeDisabled():: { execution_mode: 'Disabled' },
}
