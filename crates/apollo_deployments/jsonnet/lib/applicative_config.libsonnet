// Per-component applicative config defaults.
//
// Every key is either:
//   - a concrete value  — the intended deployment default, or
//   - overrides.<key>  — a value the deployer must supply per-environment via the `overrides` arg.
// When a new configuration field is added, add the key here with its intended deployment value, or
// reference `overrides.<key>` if it must be set per-environment.
local constants = import 'constants.libsonnet';

function(overrides)
  local chainId = overrides.chain_id;
  local ethFeeToken = overrides.eth_fee_token_address;
  local strkFeeToken = overrides.strk_fee_token_address;
  local recorderUrl = overrides.recorder_url;
  local nativeClassesWhitelist = overrides.native_classes_whitelist;
  local starknetUrl = overrides.starknet_url;
  local versionedConstantsOverrides = overrides.versioned_constants_overrides;
  local validatorId = overrides.validator_id;
  // Fixed deployment values (not in KEYS_TO_BE_REPLACED, so not supplied per-environment).
  local validateResourceBounds = true;
  local maxCpuTime = 600;
  local revertConfig = { revert_up_to_and_including: 18446744073709551615, should_revert: false };
  local behaviorMode = 'starknet';

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
      starknet_url: starknetUrl,
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
    chain_id: chainId,
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
    base_layer_config: {
      bpo1_start_block_number: overrides.base_layer_config.bpo1_start_block_number,
      bpo2_start_block_number: overrides.base_layer_config.bpo2_start_block_number,
      fusaka_no_bpo_start_block_number: overrides.base_layer_config.fusaka_no_bpo_start_block_number,
      ordered_l1_endpoint_urls: 'https://mainnet.infura.io/v3/YOUR_INFURA_API_KEY',
      retry_primary_interval_seconds: 60,
      starknet_contract_address: overrides.base_layer_config.starknet_contract_address,
      timeout_millis: 1000,
    },
    batcher_config: {
      dynamic_config: {
        n_concurrent_txs: overrides.batcher_config.dynamic_config.n_concurrent_txs,
        native_classes_whitelist: nativeClassesWhitelist,
        proposer_idle_detection_delay_millis: overrides.batcher_config.dynamic_config.proposer_idle_detection_delay_millis,
        storage_reader_server_dynamic_config: {
          enable: false,
        },
        tx_polling_interval_millis: 200,
      },
      static_config: {
        block_builder_config: {
          bouncer_config: {
            block_max_capacity: {
              l1_gas: 4400000,
              message_segment_length: 3700,
              n_events: overrides.batcher_config.static_config.block_builder_config.bouncer_config.block_max_capacity.n_events,
              n_txs: 500,
              proving_gas: 5000000000,
              receipt_l2_gas: overrides.batcher_config.static_config.block_builder_config.bouncer_config.block_max_capacity.receipt_l2_gas,
              sierra_gas: 5000000000,
              state_diff_size: overrides.batcher_config.static_config.block_builder_config.bouncer_config.block_max_capacity.state_diff_size,
            },
            builtin_instance_limits: {
              add_mod: 3000000,
              bitwise: 10500000,
              blake: 1800000,
              ecdsa: 3000,
              ecop: 130000,
              keccak: 10000,
              mul_mod: 3000000,
              pedersen: 2000000,
              poseidon: 600000,
              range_check: 66666666,
              range_check96: 33519553,
            },
          },
          chain_info: {
            chain_id: chainId,
            fee_token_addresses: {
              eth_fee_token_address: ethFeeToken,
              strk_fee_token_address: strkFeeToken,
            },
            is_l3: false,
          },
          execute_config: {
            n_workers: overrides.batcher_config.static_config.block_builder_config.execute_config.n_workers,
            stack_size: 62914560,
          },
          versioned_constants_overrides: versionedConstantsOverrides,
        },
        commitment_manager_config: {
          panic_if_task_channel_full: false,
          results_channel_size: 1000,
          tasks_channel_size: 1000,
        },
        contract_class_manager_config: {
          cairo_native_run_config: {
            cairo_native_mode: 'lazy_compilation',
            channel_size: 2000,
            panic_on_compilation_failure: false,
          },
          contract_cache_size: 2000,
          native_compiler_config: {
            compiler_binary_path: null,
            max_cpu_time: maxCpuTime,
            max_file_size: 52428800,
            max_memory_usage: 16106127360,
            optimization_level: 2,
          },
        },
        first_block_with_partial_block_hash: overrides.batcher_config.static_config.first_block_with_partial_block_hash,
        input_stream_content_buffer_size: 4000,
        max_l1_handler_txs_per_block_proposal: 200,
        outstream_content_buffer_size: 64,
        pre_confirmed_block_writer_config: {
          channel_buffer_capacity: 1000,
          write_block_interval_millis: 50,
        },
        pre_confirmed_cende_config: {
          recorder_url: recorderUrl,
        },
        propose_l1_txs_every: 10,
        storage: {
          db_config: {
            chain_id: chainId,
            enforce_file_exists: false,
            growth_step: 67108864,
            max_readers: 8192,
            max_size: 1099511627776,
            min_size: 1048576,
            path_prefix: '/data/batcher',
          },
          mmap_file_config: {
            growth_step: 2147483648,
            max_object_size: 1073741824,
            max_size: 1099511627776,
          },
          scope: 'StateOnly',
        },
        storage_reader_server_static_config: {
          ip: '0.0.0.0',
          port: 8091,
        },
        validation_only: std.get(overrides, 'validation_only', constants.DEFAULT_VALIDATION_ONLY),
      },
    },
    class_manager_config: {
      dynamic_config: {
        storage_reader_server_dynamic_config: {
          enable: false,
        },
      },
      static_config: {
        class_manager_config: {
          cached_class_storage_config: {
            class_cache_size: 128,
            deprecated_class_cache_size: 128,
          },
          max_compiled_contract_class_object_size: overrides.class_manager_config.static_config.class_manager_config.max_compiled_contract_class_object_size,
        },
        class_storage_config: {
          class_hash_storage_config: {
            db_config: {
              chain_id: chainId,
              enforce_file_exists: false,
              growth_step: 67108864,
              max_readers: 8192,
              max_size: 1099511627776,
              min_size: 1048576,
              path_prefix: '/data/class_manager/class_hash_storage',
            },
            mmap_file_config: {
              growth_step: 2147483648,
              max_object_size: 1073741824,
              max_size: 1099511627776,
            },
            scope: 'StateOnly',
          },
          persistent_root: '/data/class_manager/classes',
          storage_reader_server_static_config: {
            ip: '0.0.0.0',
            port: 8091,
          },
        },
      },
    },
    committer_config: {
      db_path: '/data/committer',
      reader_config: {
        build_storage_tries_concurrently: true,
        warn_on_trivial_modifications: false,
      },
      storage_config: {
        cache_size: overrides.committer_config.storage_config.cache_size,
        include_inner_stats: true,
        inner_storage_config: {
          bloom_filter_bits: 10,
          bytes_per_sync: 1048576,
          cache_size: overrides.committer_config.storage_config.inner_storage_config.cache_size,
          enable_statistics: true,
          max_background_jobs: 8,
          max_subcompactions: 8,
          max_write_buffers: 4,
          num_threads: 8,
          spawn_blocking_reads: true,
          use_mmap_reads: false,
          write_buffer_size: 134217728,
        },
      },
      verify_state_diff_hash: overrides.committer_config.verify_state_diff_hash,
    },
    config_manager_config: {
      config_update_interval_secs: 60.0,
      enable_config_updates: true,
    },
    consensus_manager_config: {
      assume_no_malicious_validators: true,
      broadcast_buffer_size: 10000,
      cende_config: {
        max_retry_duration_secs: 3,
        max_retry_interval_ms: 1000,
        min_retry_interval_ms: 50,
        recorder_url: recorderUrl,
      },
      consensus_manager_config: {
        dynamic_config: {
          future_msg_limit: {
            future_height_limit: 20,
            future_height_round_limit: 5,
            future_round_limit: 20,
          },
          require_virtual_proposer_vote: overrides.consensus_manager_config.consensus_manager_config.dynamic_config.require_virtual_proposer_vote,
          stop_at_height: null,
          sync_retry_interval: 1.0,
          timeouts: {
            precommit: {
              base: 1.0,
              delta: 0.5,
              max: 5.0,
            },
            prevote: {
              base: 0.3,
              delta: 0.1,
              max: 1.0,
            },
            proposal: {
              base: overrides.consensus_manager_config.consensus_manager_config.dynamic_config.timeouts.proposal.base,
              delta: 0.0,
              max: overrides.consensus_manager_config.consensus_manager_config.dynamic_config.timeouts.proposal.max,
            },
          },
          validator_id: validatorId,
        },
        static_config: {
          skip_last_voted_height_check: false,
          startup_delay: 15,
          storage_config: {
            db_config: {
              chain_id: chainId,
              enforce_file_exists: false,
              growth_step: 67108864,
              max_readers: 8192,
              max_size: 1099511627776,
              min_size: 1048576,
              path_prefix: '/data/consensus',
            },
            mmap_file_config: {
              growth_step: 2147483648,
              max_object_size: 1073741824,
              max_size: 1099511627776,
            },
            scope: 'StateOnly',
          },
        },
      },
      context_config: {
        dynamic_config: {
          build_proposal_margin_millis: overrides.consensus_manager_config.context_config.dynamic_config.build_proposal_margin_millis,
          compare_retrospective_block_hash: overrides.consensus_manager_config.context_config.dynamic_config.compare_retrospective_block_hash,
          l1_data_gas_price_multiplier_ppt: 135,
          l1_gas_tip_wei: 1000000000,
          max_l1_data_gas_price_wei: 1000000000000,
          max_l1_gas_price_wei: 1000000000000,
          min_l1_data_gas_price_wei: 1,
          min_l1_gas_price_wei: 1000000000,
          min_l2_gas_price_per_height: overrides.consensus_manager_config.context_config.dynamic_config.min_l2_gas_price_per_height,
          override_eth_to_fri_rate: overrides.consensus_manager_config.context_config.dynamic_config.override_eth_to_fri_rate,
          override_l1_data_gas_price_fri: overrides.consensus_manager_config.context_config.dynamic_config.override_l1_data_gas_price_fri,
          override_l1_gas_price_fri: overrides.consensus_manager_config.context_config.dynamic_config.override_l1_gas_price_fri,
          override_l2_gas_price_fri: overrides.consensus_manager_config.context_config.dynamic_config.override_l2_gas_price_fri,
          snip35_target_atto_usd_per_l2_gas: 880000000,
        },
        static_config: {
          behavior_mode: behaviorMode,
          block_timestamp_window_seconds: 1,
          build_proposal_time_ratio_for_retrospective_block_hash: 0.7,
          builder_address: '0x1176a1bd84444c89232ec27754698e5d2e7e1a7f1539f12027f28b23ec9f3d8',
          chain_id: chainId,
          l1_da_mode: true,
          proposal_buffer_size: 512,
          retrospective_block_hash_retry_interval_millis: 500,
          validate_proposal_margin_millis: 10000,
        },
      },
      network_config: {
        advertised_multiaddr: overrides.consensus_manager_config.network_config.advertised_multiaddr,
        bootstrap_peer_multiaddr: overrides.consensus_manager_config.network_config.bootstrap_peer_multiaddr,
        broadcasted_message_metadata_buffer_size: 100000,
        chain_id: chainId,
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
        port: overrides.consensus_manager_config.network_config.port,
        prune_dead_connections_ping_interval: 15,
        prune_dead_connections_ping_timeout: 20,
        reported_peer_ids_buffer_size: 100000,
        secret_key: '',
        session_timeout: 120,
      },
      proposals_topic: 'consensus_proposals',
      revert_config: revertConfig,
      staking_manager_config: {
        dynamic_config: {
          default_committee: overrides.consensus_manager_config.staking_manager_config.dynamic_config.default_committee,
          override_committee: overrides.consensus_manager_config.staking_manager_config.dynamic_config.override_committee,
        },
        static_config: {
          max_cached_epochs: 10,
          use_only_actual_proposer_selection: true,
        },
      },
      stream_handler_config: {
        channel_buffer_capacity: 1000,
        max_message_buffer_size: 1000,
        max_peers: 100,
        max_streams: 100,
      },
      votes_topic: 'consensus_votes',
    },
    gateway_config: {
      dynamic_config: {
        native_classes_whitelist: nativeClassesWhitelist,
      },
      static_config: {
        authorized_declarer_accounts: overrides.gateway_config.static_config.authorized_declarer_accounts,
        block_declare: false,
        chain_info: {
          chain_id: chainId,
          fee_token_addresses: {
            eth_fee_token_address: ethFeeToken,
            strk_fee_token_address: strkFeeToken,
          },
          is_l3: false,
        },
        contract_class_manager_config: {
          cairo_native_run_config: {
            cairo_native_mode: 'lazy_compilation',
            channel_size: 2000,
            panic_on_compilation_failure: false,
          },
          contract_cache_size: 300,
          native_compiler_config: {
            compiler_binary_path: null,
            max_cpu_time: maxCpuTime,
            max_file_size: 52428800,
            max_memory_usage: 16106127360,
            optimization_level: 2,
          },
        },
        max_concurrent_declare_compilations: 40,
        proof_archive_writer_config: {
          bucket_name: overrides.gateway_config.static_config.proof_archive_writer_config.bucket_name,
        },
        stateful_tx_validator_config: {
          max_allowed_nonce_gap: overrides.gateway_config.static_config.stateful_tx_validator_config.max_allowed_nonce_gap,
          max_nonce_for_validation_skip: '0x1',
          min_gas_price_percentage: 100,
          reject_future_declare_txs: true,
          validate_resource_bounds: validateResourceBounds,
          versioned_constants_overrides: versionedConstantsOverrides,
        },
        stateless_tx_validator_config: {
          allow_client_side_proving: true,
          max_calldata_length: 5000,
          max_contract_bytecode_size: overrides.gateway_config.static_config.stateless_tx_validator_config.max_contract_bytecode_size,
          max_contract_class_object_size: 4089446,
          max_l2_gas_amount: 1210000000,
          max_proof_size: 480000,
          max_sierra_version: {
            major: 1,
            minor: 9,
            patch: 0,
          },
          max_signature_length: 4000,
          min_gas_price: overrides.gateway_config.static_config.stateless_tx_validator_config.min_gas_price,
          min_sierra_version: {
            major: 1,
            minor: 1,
            patch: 0,
          },
          validate_resource_bounds: validateResourceBounds,
        },
      },
    },
    http_server_config: {
      dynamic_config: {
        accept_new_txs: true,
        max_sierra_program_size: 4194304,
      },
      static_config: {
        dynamic_config_poll_interval: 1000,
        ip: '0.0.0.0',
        max_request_body_size: 5242880,
        port: overrides.http_server_config.static_config.port,
      },
    },
    l1_events_provider_config: {
      dummy_mode: false,
      l1_handler_cancellation_timelock_seconds: 300.0,
      l1_handler_consumption_timelock_seconds: 300.0,
      l1_handler_proposal_cooldown_seconds: 70.0,
      startup_sync_sleep_retry_interval_seconds: 2.0,
    },
    l1_events_scraper_config: {
      chain_id: chainId,
      finality: 10,
      l1_block_time_seconds: 12.0,
      polling_interval_seconds: 30.0,
      set_provider_historic_height_to_l2_genesis: false,
      startup_rewind_time_seconds: 21600.0,
    },
    l1_gas_price_provider_config: {
      eth_to_strk_oracle_config: {
        lag_interval_seconds: 900,
        max_cache_size: 100,
        query_timeout_sec: 10,
        url_header_list: 'https://api.example.com/api',
      },
      lag_margin_seconds: 600.0,
      max_time_gap_seconds: 900,
      number_of_blocks_for_mean: 300,
      storage_limit: 3000,
      strk_to_usd_oracle_config: {
        lag_interval_seconds: 900,
        max_cache_size: 100,
        query_timeout_sec: 10,
        url_header_list: 'https://api.example.com/api',
      },
    },
    l1_gas_price_scraper_config: {
      chain_id: chainId,
      finality: 10,
      number_of_blocks_for_mean: 300,
      polling_interval: 120.0,
      starting_block: null,
      startup_num_blocks_multiplier: 2,
    },
    mempool_config: {
      dynamic_config: {
        transaction_ttl: overrides.mempool_config.dynamic_config.transaction_ttl,
      },
      static_config: {
        behavior_mode: behaviorMode,
        capacity_in_bytes: 1073741824,
        committed_nonce_retention_block_count: 100,
        declare_delay: 20,
        enable_fee_escalation: true,
        fee_escalation_percentage: 10,
        recorder_url: recorderUrl,
        validate_resource_bounds: validateResourceBounds,
      },
    },
    mempool_p2p_config: {
      max_concurrent_gateway_requests: 10000,
      max_transaction_batch_size: 75,
      network_buffer_size: 10000,
      network_config: {
        advertised_multiaddr: overrides.mempool_p2p_config.network_config.advertised_multiaddr,
        bootstrap_peer_multiaddr: overrides.mempool_p2p_config.network_config.bootstrap_peer_multiaddr,
        broadcasted_message_metadata_buffer_size: 100000,
        chain_id: chainId,
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
        port: overrides.mempool_p2p_config.network_config.port,
        prune_dead_connections_ping_interval: 15,
        prune_dead_connections_ping_timeout: 20,
        reported_peer_ids_buffer_size: 100000,
        secret_key: '',
        session_timeout: 120,
      },
      transaction_batch_rate_millis: 100,
    },
    monitoring_config: {
      collect_metrics: true,
      collect_profiling_metrics: true,
    },
    monitoring_endpoint_config: {
      ip: '0.0.0.0',
      port: overrides.monitoring_endpoint_config.port,
      snapshot_timeout_millis: 5000,
    },
    proof_manager_config: {
      cache_size: 500,
      persistent_root: '/data/proofs',
    },
    sierra_compiler_config: {
      audited_libfuncs_only: overrides.sierra_compiler_config.audited_libfuncs_only,
      max_bytecode_size: overrides.sierra_compiler_config.max_bytecode_size,
      max_cpu_time: maxCpuTime,
      max_memory_usage: 5368709120,
    },
    state_sync_config: {
      dynamic_config: {
        storage_reader_server_dynamic_config: {
          enable: false,
        },
      },
      static_config: {
        central_sync_client_config: optionalStateSyncSubConfig(overrides.state_sync_config.static_config.central_sync_client_config, defaultCentralSyncClientConfig),
        network_config: optionalStateSyncSubConfig(overrides.state_sync_config.static_config.network_config, defaultStateSyncNetworkConfig),
        p2p_sync_client_config: optionalStateSyncSubConfig(overrides.state_sync_config.static_config.p2p_sync_client_config, defaultP2pSyncClientConfig),
        revert_config: revertConfig,
        rpc_config: {
          apollo_gateway_retry_config: {
            max_retries: 10,
            retry_base_millis: 30,
            retry_max_delay_millis: 1000,
          },
          chain_id: chainId,
          collect_metrics: false,
          execution_config: {
            default_initial_gas_cost: 10000000000,
            eth_fee_contract_address: ethFeeToken,
            strk_fee_contract_address: strkFeeToken,
          },
          ip: '0.0.0.0',
          max_events_chunk_size: 1000,
          max_events_keys: 100,
          port: overrides.state_sync_config.static_config.rpc_config.port,
          starknet_url: starknetUrl,
        },
        storage_config: {
          db_config: {
            chain_id: chainId,
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
    },
  }
