local constants = import '../constants.libsonnet';

function(chain_params, replacers)
  local chainId = chain_params.chain_id;
  local ethFeeToken = std.get(replacers, 'eth_fee_token_address', constants.DEFAULT_ETH_FEE_TOKEN_ADDRESS);
  local strkFeeToken = std.get(replacers, 'strk_fee_token_address', constants.DEFAULT_STRK_FEE_TOKEN_ADDRESS);
  local recorderUrl = chain_params.recorder_url;
  local nativeClassesWhitelist = chain_params.native_classes_whitelist;
  local versionedConstantsOverrides = std.get(replacers, 'versioned_constants_overrides', null);
  local maxCpuTime = 600;
  {
    dynamic_config: {
      n_concurrent_txs: std.get(replacers, 'batcher_config.dynamic_config.n_concurrent_txs', 100),
      native_classes_whitelist: nativeClassesWhitelist,
      proposer_idle_detection_delay_millis: std.get(replacers, 'batcher_config.dynamic_config.proposer_idle_detection_delay_millis', 1500),
      storage_reader_server_dynamic_config: {
        enable: false,
      },
      tx_polling_interval_millis: 200,
      validate_tx_polling_interval_millis: 10,
    },
    static_config: {
      block_builder_config: {
        bouncer_config: {
          block_max_capacity: {
            l1_gas: 4400000,
            message_segment_length: 3700,
            n_events: std.get(replacers, 'batcher_config.static_config.block_builder_config.bouncer_config.block_max_capacity.n_events', 5000),
            n_txs: 500,
            proving_gas: 5000000000,
            receipt_l2_gas: std.get(replacers, 'batcher_config.static_config.block_builder_config.bouncer_config.block_max_capacity.receipt_l2_gas', 5800000000),
            sierra_gas: 5000000000,
            state_diff_size: std.get(replacers, 'batcher_config.static_config.block_builder_config.bouncer_config.block_max_capacity.state_diff_size', 4000),
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
          n_workers: std.get(replacers, 'batcher_config.static_config.block_builder_config.execute_config.n_workers', 5),
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
      first_block_with_partial_block_hash: chain_params.batcher_config.static_config.first_block_with_partial_block_hash,
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
        port: 55011,
      },
      validation_only: std.get(replacers, 'validation_only', constants.DEFAULT_VALIDATION_ONLY),
    },
  }
