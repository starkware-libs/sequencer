function(chain_params)
  {
    max_concurrent_gateway_requests: 10000,
    max_transaction_batch_size: 75,
    network_buffer_size: 10000,
    network_config: {
      advertised_multiaddr: chain_params.mempool_advertised_multiaddr,
      bootstrap_peer_multiaddr: chain_params.mempool_bootstrap_peer_multiaddr,
      broadcasted_message_metadata_buffer_size: 100000,
      chain_id: chain_params.chain_id,
      discovery_config: {
        bootstrap_dial_retry_config: {
          base_delay_millis: 2,
          factor: 5,
          max_delay_seconds: 5,
          new_connection_stabilization_millis: 2000,
        },
        heartbeat_interval: 100,
      },
      idle_connection_timeout: 120,
      peer_manager_config: {
        malicious_timeout_seconds: 0,
        unstable_timeout_millis: 0,
      },
      port: 53200,
      prune_dead_connections_ping_interval: 15,
      prune_dead_connections_ping_timeout: 20,
      reported_peer_ids_buffer_size: 100000,
      secret_key: '',
      session_timeout: 120,
    },
    transaction_batch_rate_millis: 100,
  }
