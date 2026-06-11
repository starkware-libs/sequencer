function(replacers)
  {
    dynamic_config: {
      accept_new_txs: true,
      max_sierra_program_size: 4194304,
    },
    static_config: {
      dynamic_config_poll_interval: 1000,
      ip: '0.0.0.0',
      max_request_body_size: 5242880,
      port: replacers['http_server_config.static_config.port'],
    },
  }
