local defaults = import 'lib/defaults.libsonnet';
function(chain_params)
  // Handle pointers that are gated under optional configurations.
  local optionalStateSyncSubConfig(value, default) =
    if value == null then null else std.mergePatch(default, value);
  local defaultCentralSyncClientConfig = {
    central_source_config: {
      class_cache_size: 128,
      concurrent_requests: 20,
      http_headers: '',
      max_classes_to_download: 20,
      max_state_updates_to_download: 20,
      max_state_updates_to_store_in_memory: 20,
      retry_config: { max_retries: 10, retry_base_millis: 30, retry_max_delay_millis: 30000 },
      starknet_url: chain_params.starknet_url,
    },
    sync_config: {
      base_layer_propagation_sleep_duration: 10,
      blocks_before_tip_to_disable_batching: 100,
      blocks_max_stream_size: 1000,
      collect_pending_data: false,
      latest_block_poll_interval_millis: 500,
      recoverable_error_sleep_duration: 3,
      state_updates_max_stream_size: 1000,
      store_sierras_and_casms_block_threshold: 0,
      verify_blocks: false,
    },
  };
  local defaultStateSyncNetworkConfig = {
    advertised_multiaddr: null,
    bootstrap_peer_multiaddr: null,
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
    peer_manager_config: { malicious_timeout_seconds: 1, unstable_timeout_millis: 1000 },
    port: 53140,
    prune_dead_connections_ping_interval: 15,
    prune_dead_connections_ping_timeout: 20,
    reported_peer_ids_buffer_size: 100000,
    secret_key: '',
    session_timeout: 120,
  };
  local defaultP2pSyncClientConfig = {
    buffer_size: 100000,
    num_block_classes_per_query: 100,
    num_block_state_diffs_per_query: 100,
    num_block_transactions_per_query: 100,
    num_headers_per_query: 10000,
    wait_period_for_new_data: 50,
    wait_period_for_other_protocol: 50,
  };
  {
    dynamic_config: {
      storage_reader_server_dynamic_config: {
        enable: false,
      },
    },
    static_config: {
      // Default `{}` (not null) enables central sync with all defaults, so a minimal deployment
      // satisfies the "exactly one of central/p2p sync client" invariant; an overlay sets null to
      // disable it (and enable p2p instead) or a partial object to tweak it.
      central_sync_client_config: optionalStateSyncSubConfig(chain_params.central_sync_client_config, defaultCentralSyncClientConfig),
      network_config: optionalStateSyncSubConfig(chain_params.state_sync_network_config, defaultStateSyncNetworkConfig),
      p2p_sync_client_config: optionalStateSyncSubConfig(chain_params.p2p_sync_client_config, defaultP2pSyncClientConfig),
      revert_config: defaults.REVERT_CONFIG,
      rpc_config: {
        apollo_gateway_retry_config: {
          max_retries: 10,
          retry_base_millis: 30,
          retry_max_delay_millis: 1000,
        },
        chain_id: chain_params.chain_id,
        collect_metrics: false,
        execution_config: {
          default_initial_gas_cost: 10000000000,
          eth_fee_contract_address: chain_params.eth_fee_token_address,
          strk_fee_contract_address: chain_params.strk_fee_token_address,
        },
        ip: '0.0.0.0',
        max_events_chunk_size: 1000,
        max_events_keys: 100,
        port: 8090,
        starknet_url: chain_params.starknet_url,
      },
      storage_config: {
        db_config: {
          chain_id: chain_params.chain_id,
          enforce_file_exists: false,
          growth_step: 67108864,
          max_readers: 8192,
          max_size: 1099511627776,
          min_size: 1048576,
          path_prefix: '/data/state_sync',
        },
        mmap_file_config: {
          growth_step: 2147483648,
          max_object_size: 1073741824,
          max_size: 1099511627776,
        },
        scope: 'FullArchive',
      },
      storage_reader_server_static_config: {
        ip: '0.0.0.0',
        port: 8091,
      },
    },
  }
