function(replacers)
  {
    ip: '0.0.0.0',
    port: std.get(replacers, 'monitoring_endpoint_config.port', 8082),
    snapshot_timeout_millis: 5000,
  }
