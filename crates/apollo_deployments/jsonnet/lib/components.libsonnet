// Per-component config defaults and required-override markers.
//
// Every key is either:
//   - a concrete value  — the intended deployment default, or
//   - error "must be set by node config"  — must be overridden before Jsonnet evaluates.
//
// A Rust test enforces this invariant: every key must equal its Rust default value unless
// it is in KEYS_TO_BE_REPLACED (in which case the error marker is required).
// When a new Rust config field is added, that test fails. Fix by adding the key here with
// its intended deployment default, or with error if it must be set per-environment.
local mustOverride = error "must be set by node config";
{
  base_layer: {
    base_layer_config: {
      bpo1_start_block_number: mustOverride,
      bpo2_start_block_number: mustOverride,
      fusaka_no_bpo_start_block_number: mustOverride,
      ordered_l1_endpoint_urls: "https://mainnet.infura.io/v3/YOUR_INFURA_API_KEY",
      starknet_contract_address: mustOverride,
      timeout_millis: 1000,
    },
  },
  batcher: {
    batcher_config: {
      dynamic_config: {
        n_concurrent_txs: mustOverride,
        native_classes_whitelist: mustOverride,
        proposer_idle_detection_delay_millis: mustOverride,
        storage_reader_server_dynamic_config: {
          enable: false,
        },
        tx_polling_interval_millis: 10,
      },
      static_config: {
        block_builder_config: {
          bouncer_config: {
            block_max_capacity: {
              l1_gas: 2500000,
              message_segment_length: 3700,
              n_events: mustOverride,
              n_txs: 600,
              proving_gas: 5000000000,
              receipt_l2_gas: mustOverride,
              sierra_gas: 5000000000,
              state_diff_size: mustOverride,
            },
            builtin_weights: {
              gas_costs: {
                add_mod: 2000,
                bitwise: 583,
                blake: 3334,
                ecdsa: 2000000,
                ecop: 857850,
                keccak: 600000,
                mul_mod: 2000,
                pedersen: 3000,
                poseidon: 10000,
                range_check: 90,
                range_check96: 179,
              },
            },
          },
          chain_info: {
            chain_id: mustOverride,
            fee_token_addresses: {
              eth_fee_token_address: mustOverride,
              strk_fee_token_address: mustOverride,
            },
            is_l3: false,
          },
          execute_config: {
            n_workers: mustOverride,
            stack_size: 62914560,
          },
          versioned_constants_overrides: mustOverride,
        },
        commitment_manager_config: {
          panic_if_task_channel_full: false,
          results_channel_size: 1000,
          tasks_channel_size: 1000,
        },
        contract_class_manager_config: {
          cairo_native_run_config: {
            cairo_native_mode: "off",
            channel_size: 2000,
            panic_on_compilation_failure: false,
          },
          contract_cache_size: 600,
          native_compiler_config: {
            compiler_binary_path: null,
            max_cpu_time: mustOverride,
            max_file_size: 52428800,
            max_memory_usage: 16106127360,
            optimization_level: 2,
          },
        },
        first_block_with_partial_block_hash: mustOverride,
        input_stream_content_buffer_size: 400,
        max_l1_handler_txs_per_block_proposal: 3,
        outstream_content_buffer_size: 100,
        pre_confirmed_block_writer_config: {
          channel_buffer_capacity: 1000,
          write_block_interval_millis: 50,
        },
        pre_confirmed_cende_config: {
          recorder_url: mustOverride,
        },
        propose_l1_txs_every: 1,
        storage: {
          db_config: {
            chain_id: mustOverride,
            enforce_file_exists: false,
            growth_step: 4294967296,
            max_readers: 8192,
            max_size: 1099511627776,
            min_size: 1048576,
            path_prefix: "/data/batcher",
          },
          mmap_file_config: {
            growth_step: 1073741824,
            max_object_size: 268435456,
            max_size: 1099511627776,
          },
          scope: "StateOnly",
        },
        storage_reader_server_static_config: {
          ip: "0.0.0.0",
          port: 8091,
        },
        validation_only: mustOverride,
      },
    },
  },
  class_manager: {
    class_manager_config: {
      dynamic_config: {
        storage_reader_server_dynamic_config: {
          enable: false,
        },
      },
      static_config: {
        class_manager_config: {
          cached_class_storage_config: {
            class_cache_size: 10,
            deprecated_class_cache_size: 10,
          },
          max_compiled_contract_class_object_size: mustOverride,
        },
        class_storage_config: {
          class_hash_storage_config: {
            db_config: {
              chain_id: mustOverride,
              enforce_file_exists: false,
              growth_step: 4294967296,
              max_readers: 8192,
              max_size: 1099511627776,
              min_size: 1048576,
              path_prefix: "/data/class_hash_storage",
            },
            mmap_file_config: {
              growth_step: 1048576,
              max_object_size: 1024,
              max_size: 1073741824,
            },
            scope: "StateOnly",
          },
          persistent_root: "/data/classes",
          storage_reader_server_static_config: {
            ip: "0.0.0.0",
            port: 8091,
          },
        },
      },
    },
  },
  committer: {
    committer_config: {
      db_path: "/data/committer",
      reader_config: {
        build_storage_tries_concurrently: true,
        warn_on_trivial_modifications: false,
      },
      storage_config: {
        cache_size: mustOverride,
        include_inner_stats: true,
        inner_storage_config: {
          bloom_filter_bits: 10,
          bytes_per_sync: 1048576,
          cache_size: 8589934592,
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
      verify_state_diff_hash: mustOverride,
    },
  },
  config_manager: {
    config_manager_config: {
      config_update_interval_secs: 60.0,
      enable_config_updates: false,
    },
  },
  consensus_manager: {
    consensus_manager_config: {
      assume_no_malicious_validators: false,
      broadcast_buffer_size: 10000,
      cende_config: {
        max_retry_duration_secs: 3,
        max_retry_interval_ms: 1000,
        min_retry_interval_ms: 50,
        recorder_url: mustOverride,
      },
      consensus_manager_config: {
        dynamic_config: {
          future_msg_limit: {
            future_height_limit: 10,
            future_height_round_limit: 1,
            future_round_limit: 10,
          },
          require_virtual_proposer_vote: mustOverride,
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
              base: mustOverride,
              delta: 0.0,
              max: mustOverride,
            },
          },
          validator_id: mustOverride,
        },
        static_config: {
          skip_last_voted_height_check: false,
          startup_delay: 5,
          storage_config: {
            db_config: {
              chain_id: mustOverride,
              enforce_file_exists: false,
              growth_step: 4294967296,
              max_readers: 8192,
              max_size: 1099511627776,
              min_size: 1048576,
              path_prefix: "/data/consensus",
            },
            mmap_file_config: {
              growth_step: 1073741824,
              max_object_size: 268435456,
              max_size: 1099511627776,
            },
            scope: "StateOnly",
          },
        },
      },
      context_config: {
        dynamic_config: {
          build_proposal_margin_millis: mustOverride,
          compare_retrospective_block_hash: mustOverride,
          l1_data_gas_price_multiplier_ppt: 135,
          l1_gas_tip_wei: 1000000000,
          max_l1_data_gas_price_wei: 1000000000000000000,
          max_l1_gas_price_wei: 200000000000,
          min_l1_data_gas_price_wei: 1,
          min_l1_gas_price_wei: 1000000000,
          min_l2_gas_price_per_height: mustOverride,
          override_eth_to_fri_rate: mustOverride,
          override_l1_data_gas_price_fri: mustOverride,
          override_l1_gas_price_fri: mustOverride,
          override_l2_gas_price_fri: mustOverride,
        },
        static_config: {
          behavior_mode: mustOverride,
          block_timestamp_window_seconds: 1,
          build_proposal_time_ratio_for_retrospective_block_hash: 0.7,
          builder_address: "0x0",
          chain_id: mustOverride,
          l1_da_mode: true,
          proposal_buffer_size: 100,
          retrospective_block_hash_retry_interval_millis: 500,
          validate_proposal_margin_millis: 10000,
        },
      },
      network_config: {
        advertised_multiaddr: mustOverride,
        bootstrap_peer_multiaddr: mustOverride,
        broadcasted_message_metadata_buffer_size: 100000,
        chain_id: mustOverride,
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
          malicious_timeout_seconds: 1,
          unstable_timeout_millis: 1000,
        },
        port: mustOverride,
        prune_dead_connections_ping_interval: 15,
        prune_dead_connections_ping_timeout: 20,
        reported_peer_ids_buffer_size: 100000,
        secret_key: "",
        session_timeout: 120,
      },
      proposals_topic: "consensus_proposals",
      revert_config: {
        revert_up_to_and_including: mustOverride,
        should_revert: mustOverride,
      },
      staking_manager_config: {
        dynamic_config: {
          default_committee: mustOverride,
          override_committee: mustOverride,
        },
        static_config: {
          max_cached_epochs: 10,
          use_only_actual_proposer_selection: false,
        },
      },
      stream_handler_config: {
        channel_buffer_capacity: 1000,
        max_message_buffer_size: 1000,
        max_peers: 100,
        max_streams: 100,
      },
      votes_topic: "consensus_votes",
    },
  },
  gateway: {
    gateway_config: {
      dynamic_config: {
        native_classes_whitelist: mustOverride,
      },
      static_config: {
        authorized_declarer_accounts: mustOverride,
        block_declare: false,
        chain_info: {
          chain_id: mustOverride,
          fee_token_addresses: {
            eth_fee_token_address: mustOverride,
            strk_fee_token_address: mustOverride,
          },
          is_l3: false,
        },
        contract_class_manager_config: {
          cairo_native_run_config: {
            cairo_native_mode: "off",
            channel_size: 2000,
            panic_on_compilation_failure: false,
          },
          contract_cache_size: 300,
          native_compiler_config: {
            compiler_binary_path: null,
            max_cpu_time: mustOverride,
            max_file_size: 52428800,
            max_memory_usage: 16106127360,
            optimization_level: 2,
          },
        },
        proof_archive_writer_config: {
          bucket_name: mustOverride,
        },
        stateful_tx_validator_config: {
          max_allowed_nonce_gap: mustOverride,
          max_nonce_for_validation_skip: "0x1",
          min_gas_price_percentage: 100,
          reject_future_declare_txs: true,
          validate_resource_bounds: mustOverride,
          versioned_constants_overrides: mustOverride,
        },
        stateless_tx_validator_config: {
          allow_client_side_proving: false,
          max_calldata_length: 5000,
          max_contract_bytecode_size: mustOverride,
          max_contract_class_object_size: 4089446,
          max_l2_gas_amount: 1200000000,
          max_proof_size: 480000,
          max_sierra_version: {
            major: 1,
            minor: 8,
            patch: 18446744073709551615,
          },
          max_signature_length: 4000,
          min_gas_price: mustOverride,
          min_sierra_version: {
            major: 1,
            minor: 1,
            patch: 0,
          },
          validate_resource_bounds: mustOverride,
        },
      },
    },
  },
  general: {
    components: {
      batcher: {
        execution_mode: "LocalExecutionWithRemoteDisabled",
        local_server_config: {
          high_priority_requests_channel_capacity: 1024,
          inbound_requests_channel_capacity: 1024,
          max_concurrency: 128,
          normal_priority_requests_channel_capacity: 1024,
          processing_time_warning_threshold_ms: 3000,
        },
        port: 0,
        remote_client_config: null,
        remote_server_config: null,
        url: "localhost",
      },
      class_manager: {
        execution_mode: "LocalExecutionWithRemoteDisabled",
        local_server_config: {
          high_priority_requests_channel_capacity: 1024,
          inbound_requests_channel_capacity: 1024,
          max_concurrency: 128,
          normal_priority_requests_channel_capacity: 1024,
          processing_time_warning_threshold_ms: 3000,
        },
        port: 0,
        remote_client_config: null,
        remote_server_config: null,
        url: "localhost",
      },
      committer: {
        execution_mode: "LocalExecutionWithRemoteDisabled",
        local_server_config: {
          high_priority_requests_channel_capacity: 1024,
          inbound_requests_channel_capacity: 1024,
          max_concurrency: 128,
          normal_priority_requests_channel_capacity: 1024,
          processing_time_warning_threshold_ms: 3000,
        },
        port: 0,
        remote_client_config: null,
        remote_server_config: null,
        url: "localhost",
      },
      config_manager: {
        execution_mode: "LocalExecutionWithRemoteDisabled",
        local_server_config: {
          high_priority_requests_channel_capacity: 1024,
          inbound_requests_channel_capacity: 1024,
          max_concurrency: 128,
          normal_priority_requests_channel_capacity: 1024,
          processing_time_warning_threshold_ms: 3000,
        },
        port: 0,
        remote_client_config: null,
        remote_server_config: null,
        url: "localhost",
      },
      consensus_manager: {
        execution_mode: "Enabled",
      },
      gateway: {
        execution_mode: "LocalExecutionWithRemoteDisabled",
        local_server_config: {
          high_priority_requests_channel_capacity: 1024,
          inbound_requests_channel_capacity: 1024,
          max_concurrency: 128,
          normal_priority_requests_channel_capacity: 1024,
          processing_time_warning_threshold_ms: 3000,
        },
        port: 0,
        remote_client_config: null,
        remote_server_config: null,
        url: "localhost",
      },
      http_server: {
        execution_mode: "Enabled",
      },
      l1_events_provider: {
        execution_mode: "LocalExecutionWithRemoteDisabled",
        local_server_config: {
          high_priority_requests_channel_capacity: 1024,
          inbound_requests_channel_capacity: 1024,
          max_concurrency: 128,
          normal_priority_requests_channel_capacity: 1024,
          processing_time_warning_threshold_ms: 3000,
        },
        port: 0,
        remote_client_config: null,
        remote_server_config: null,
        url: "localhost",
      },
      l1_events_scraper: {
        execution_mode: "Enabled",
      },
      l1_gas_price_provider: {
        execution_mode: "LocalExecutionWithRemoteDisabled",
        local_server_config: {
          high_priority_requests_channel_capacity: 1024,
          inbound_requests_channel_capacity: 1024,
          max_concurrency: 128,
          normal_priority_requests_channel_capacity: 1024,
          processing_time_warning_threshold_ms: 3000,
        },
        port: 0,
        remote_client_config: null,
        remote_server_config: null,
        url: "localhost",
      },
      l1_gas_price_scraper: {
        execution_mode: "Enabled",
      },
      mempool: {
        execution_mode: "LocalExecutionWithRemoteDisabled",
        local_server_config: {
          high_priority_requests_channel_capacity: 1024,
          inbound_requests_channel_capacity: 1024,
          max_concurrency: 128,
          normal_priority_requests_channel_capacity: 1024,
          processing_time_warning_threshold_ms: 3000,
        },
        port: 0,
        remote_client_config: null,
        remote_server_config: null,
        url: "localhost",
      },
      mempool_p2p: {
        execution_mode: "LocalExecutionWithRemoteDisabled",
        local_server_config: {
          high_priority_requests_channel_capacity: 1024,
          inbound_requests_channel_capacity: 1024,
          max_concurrency: 128,
          normal_priority_requests_channel_capacity: 1024,
          processing_time_warning_threshold_ms: 3000,
        },
        port: 0,
        remote_client_config: null,
        remote_server_config: null,
        url: "localhost",
      },
      monitoring_endpoint: {
        execution_mode: "Enabled",
      },
      proof_manager: {
        execution_mode: "LocalExecutionWithRemoteDisabled",
        local_server_config: {
          high_priority_requests_channel_capacity: 1024,
          inbound_requests_channel_capacity: 1024,
          max_concurrency: 128,
          normal_priority_requests_channel_capacity: 1024,
          processing_time_warning_threshold_ms: 3000,
        },
        port: 0,
        remote_client_config: null,
        remote_server_config: null,
        url: "localhost",
      },
      sierra_compiler: {
        execution_mode: "LocalExecutionWithRemoteDisabled",
        local_server_config: {
          high_priority_requests_channel_capacity: 1024,
          inbound_requests_channel_capacity: 1024,
          max_concurrency: 128,
          normal_priority_requests_channel_capacity: 1024,
          processing_time_warning_threshold_ms: 3000,
        },
        port: 0,
        remote_client_config: null,
        remote_server_config: null,
        url: "localhost",
      },
      signature_manager: {
        execution_mode: "LocalExecutionWithRemoteDisabled",
        local_server_config: {
          high_priority_requests_channel_capacity: 1024,
          inbound_requests_channel_capacity: 1024,
          max_concurrency: 128,
          normal_priority_requests_channel_capacity: 1024,
          processing_time_warning_threshold_ms: 3000,
        },
        port: 0,
        remote_client_config: null,
        remote_server_config: null,
        url: "localhost",
      },
      state_sync: {
        execution_mode: "LocalExecutionWithRemoteDisabled",
        local_server_config: {
          high_priority_requests_channel_capacity: 1024,
          inbound_requests_channel_capacity: 1024,
          max_concurrency: 128,
          normal_priority_requests_channel_capacity: 1024,
          processing_time_warning_threshold_ms: 3000,
        },
        port: 0,
        remote_client_config: null,
        remote_server_config: null,
        url: "localhost",
      },
    },
    monitoring_config: {
      collect_metrics: true,
      collect_profiling_metrics: true,
    },
    validation_only: false,
  },
  http_server: {
    http_server_config: {
      dynamic_config: {
        accept_new_txs: true,
        max_sierra_program_size: 4194304,
      },
      static_config: {
        dynamic_config_poll_interval: 1000,
        ip: "0.0.0.0",
        max_request_body_size: 5242880,
        port: mustOverride,
      },
    },
  },
  l1_events_provider: {
    l1_events_provider_config: {
      dummy_mode: false,
      l1_handler_cancellation_timelock_seconds: 300.0,
      l1_handler_consumption_timelock_seconds: 300.0,
      l1_handler_proposal_cooldown_seconds: 70.0,
      startup_sync_sleep_retry_interval_seconds: 2.0,
    },
  },
  l1_events_scraper: {
    l1_events_scraper_config: {
      chain_id: mustOverride,
      finality: 0,
      l1_block_time_seconds: 12.0,
      polling_interval_seconds: 30.0,
      set_provider_historic_height_to_l2_genesis: false,
      startup_rewind_time_seconds: 3600.0,
    },
  },
  l1_gas_price_provider: {
    l1_gas_price_provider_config: {
      eth_to_strk_oracle_config: {
        lag_interval_seconds: 1,
        max_cache_size: 100,
        query_timeout_sec: 10,
        url_header_list: "https://api.example.com/api",
      },
      lag_margin_seconds: 60.0,
      max_time_gap_seconds: 900,
      number_of_blocks_for_mean: 300,
      storage_limit: 3000,
    },
  },
  l1_gas_price_scraper: {
    l1_gas_price_scraper_config: {
      chain_id: mustOverride,
      finality: 0,
      number_of_blocks_for_mean: 300,
      polling_interval: 1.0,
      starting_block: null,
      startup_num_blocks_multiplier: 2,
    },
  },
  mempool: {
    mempool_config: {
      dynamic_config: {
        transaction_ttl: mustOverride,
      },
      static_config: {
        behavior_mode: mustOverride,
        capacity_in_bytes: 1073741824,
        committed_nonce_retention_block_count: 100,
        declare_delay: 1,
        enable_fee_escalation: true,
        fee_escalation_percentage: 10,
        recorder_url: mustOverride,
        validate_resource_bounds: mustOverride,
      },
    },
  },
  mempool_p2p: {
    mempool_p2p_config: {
      max_concurrent_gateway_requests: 10000,
      max_transaction_batch_size: 1,
      network_buffer_size: 10000,
      network_config: {
        advertised_multiaddr: mustOverride,
        bootstrap_peer_multiaddr: mustOverride,
        broadcasted_message_metadata_buffer_size: 100000,
        chain_id: mustOverride,
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
          malicious_timeout_seconds: 1,
          unstable_timeout_millis: 1000,
        },
        port: mustOverride,
        prune_dead_connections_ping_interval: 15,
        prune_dead_connections_ping_timeout: 20,
        reported_peer_ids_buffer_size: 100000,
        secret_key: "",
        session_timeout: 120,
      },
      transaction_batch_rate_millis: 1000,
    },
  },
  monitoring_endpoint: {
    monitoring_endpoint_config: {
      ip: "0.0.0.0",
      port: mustOverride,
      snapshot_timeout_millis: 5000,
    },
  },
  proof_manager: {
    proof_manager_config: {
      cache_size: 500,
      persistent_root: "/data/proofs",
    },
  },
  sierra_compiler: {
    sierra_compiler_config: {
      audited_libfuncs_only: mustOverride,
      max_bytecode_size: mustOverride,
      max_cpu_time: mustOverride,
      max_memory_usage: 5368709120,
    },
  },
  signature_manager: {
  },
  state_sync: {
    state_sync_config: {
      dynamic_config: {
        storage_reader_server_dynamic_config: {
          enable: false,
        },
      },
      static_config: {
        central_sync_client_config: mustOverride,
        network_config: mustOverride,
        p2p_sync_client_config: mustOverride,
        revert_config: {
          revert_up_to_and_including: mustOverride,
          should_revert: mustOverride,
        },
        rpc_config: {
          apollo_gateway_retry_config: {
            max_retries: 5,
            retry_base_millis: 50,
            retry_max_delay_millis: 1000,
          },
          chain_id: mustOverride,
          collect_metrics: false,
          execution_config: {
            default_initial_gas_cost: 10000000000,
            eth_fee_contract_address: mustOverride,
            strk_fee_contract_address: mustOverride,
          },
          ip: "0.0.0.0",
          max_events_chunk_size: 1000,
          max_events_keys: 100,
          port: mustOverride,
          starknet_url: mustOverride,
        },
        storage_config: {
          db_config: {
            chain_id: mustOverride,
            enforce_file_exists: false,
            growth_step: 4294967296,
            max_readers: 8192,
            max_size: 1099511627776,
            min_size: 1048576,
            path_prefix: "/data/state_sync",
          },
          mmap_file_config: {
            growth_step: 1073741824,
            max_object_size: 268435456,
            max_size: 1099511627776,
          },
          scope: "FullArchive",
        },
        storage_reader_server_static_config: {
          ip: "0.0.0.0",
          port: 8091,
        },
      },
    },
  },
}
