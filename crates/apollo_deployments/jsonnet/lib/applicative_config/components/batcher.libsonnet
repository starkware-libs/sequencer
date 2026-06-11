local defaults = import 'lib/defaults.libsonnet';
function(chain_params)
  {
    dynamic_config: {
      n_concurrent_txs: chain_params.n_concurrent_txs,
      native_classes_whitelist: chain_params.native_classes_whitelist,
      proposer_idle_detection_delay_millis: chain_params.proposer_idle_detection_delay_millis,
      storage_reader_server_dynamic_config: {
        enable: false,
      },
      results_polling_interval_millis: 10,
      tx_polling_interval_millis: 200,
      validate_tx_polling_interval_millis: 10,
      view_call_timeout_millis: 5000,
    },
    static_config: {
      block_builder_config: {
        bouncer_config: {
          block_max_capacity: {
            l1_gas: 4400000,
            message_segment_length: 3700,
            n_events: chain_params.max_events_in_block,
            n_txs: 500,
            proving_gas: 5000000000,
            receipt_l2_gas: chain_params.max_receipt_l2_gas_in_block,
            sierra_gas: 5000000000,
            state_diff_size: chain_params.max_state_diff_in_block,
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
          chain_id: chain_params.mandatory.chain_id,
          fee_token_addresses: {
            eth_fee_token_address: chain_params.eth_fee_token_address,
            strk_fee_token_address: chain_params.strk_fee_token_address,
          },
          is_l3: false,
        },
        execute_config: {
          n_workers: chain_params.n_execution_workers,
          stack_size: 62914560,
        },
        versioned_constants_overrides: chain_params.versioned_constants_overrides,
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
          max_cpu_time: defaults.MAX_CPU_TIME,
          max_file_size: 52428800,
          max_memory_usage: 16106127360,
          optimization_level: 2,
        },
      },
      first_block_with_partial_block_hash: chain_params.first_block_with_partial_block_hash,
      input_stream_content_buffer_size: 4000,
      max_l1_handler_txs_per_block_proposal: 200,
      outstream_content_buffer_size: 64,
      pre_confirmed_block_writer_config: {
        channel_buffer_capacity: 1000,
        write_block_interval_millis: 50,
      },
      pre_confirmed_cende_config: {
        recorder_url: chain_params.mandatory.recorder_url,
      },
      propose_l1_txs_every: 10,
      storage: {
        db_config: {
          chain_id: chain_params.mandatory.chain_id,
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
      validation_only: chain_params.validation_only,
    },
  }
