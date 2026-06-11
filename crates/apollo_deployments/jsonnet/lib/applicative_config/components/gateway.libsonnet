local defaults = import 'lib/defaults.libsonnet';
function(chain_params)
  {
    dynamic_config: {
      native_classes_whitelist: chain_params.native_classes_whitelist,
    },
    static_config: {
      authorized_declarer_accounts: chain_params.authorized_declarer_accounts,
      block_declare: false,
      chain_info: {
        chain_id: chain_params.mandatory.chain_id,
        fee_token_addresses: {
          eth_fee_token_address: chain_params.eth_fee_token_address,
          strk_fee_token_address: chain_params.strk_fee_token_address,
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
          max_cpu_time: defaults.MAX_CPU_TIME,
          max_file_size: 52428800,
          max_memory_usage: 16106127360,
          optimization_level: 2,
        },
      },
      max_concurrent_declare_compilations: 40,
      proof_archive_writer_config: {
        bucket_name: chain_params.mandatory.proof_archive_bucket_name,
      },
      stateful_tx_validator_config: {
        max_allowed_nonce_gap: chain_params.max_allowed_nonce_gap,
        max_nonce_for_validation_skip: '0x1',
        min_gas_price_percentage: 100,
        reject_future_declare_txs: true,
        validate_resource_bounds: defaults.VALIDATE_RESOURCE_BOUNDS,
        versioned_constants_overrides: chain_params.versioned_constants_overrides,
      },
      stateless_tx_validator_config: {
        allow_client_side_proving: true,
        max_calldata_length: 5000,
        max_contract_bytecode_size: chain_params.max_contract_bytecode_size,
        max_contract_class_object_size: 4089446,
        max_l2_gas_amount: 1210000000,
        max_proof_size: 480000,
        max_sierra_version: {
          major: 1,
          minor: 9,
          patch: 0,
        },
        max_signature_length: 4000,
        min_gas_price: chain_params.min_gas_price,
        min_sierra_version: {
          major: 1,
          minor: 1,
          patch: 0,
        },
        validate_resource_bounds: defaults.VALIDATE_RESOURCE_BOUNDS,
      },
    },
  }
