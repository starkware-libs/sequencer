// Testing `overrides` that satisfies the applicative config's schema.
{
  // Cross-cutting values (the CONFIG_POINTERS targets), referenced as flat keys.
  chain_id: 'SN_SEPOLIA',
  eth_fee_token_address: '0x1',
  strk_fee_token_address: '0x2',
  recorder_url: 'https://recorder_url',
  starknet_url: 'https://starknet_url/',
  native_classes_whitelist: '[]',
  validator_id: '0x64',
  versioned_constants_overrides: null,

  // Per-component required values, at their exact schema paths.
  base_layer_config: {
    bpo1_start_block_number: 9456501,
    bpo2_start_block_number: 9504747,
    fusaka_no_bpo_start_block_number: 9408577,
    starknet_contract_address: '0x0000000000000000000000000000000000000001',
  },
  batcher_config: {
    dynamic_config: {
      n_concurrent_txs: 100,
      proposer_idle_detection_delay_millis: 1500,
    },
    static_config: {
      block_builder_config: {
        bouncer_config: {
          block_max_capacity: {
            n_events: 5000,
            receipt_l2_gas: 5800000000,
            state_diff_size: 4000,
          },
        },
        execute_config: {
          n_workers: 28,
        },
      },
      first_block_with_partial_block_hash: null,
    },
  },
  class_manager_config: {
    static_config: {
      class_manager_config: {
        max_compiled_contract_class_object_size: 4089446,
      },
    },
  },
  committer_config: {
    storage_config: {
      cache_size: 10000000,
      inner_storage_config: {
        cache_size: 8589934592,
      },
    },
    verify_state_diff_hash: true,
  },
  consensus_manager_config: {
    consensus_manager_config: {
      dynamic_config: {
        require_virtual_proposer_vote: false,
        timeouts: {
          proposal: {
            base: 9.1,
            max: 15.0,
          },
        },
      },
    },
    context_config: {
      dynamic_config: {
        build_proposal_margin_millis: 1000,
        compare_retrospective_block_hash: false,
        min_l2_gas_price_per_height: '',
        override_eth_to_fri_rate: null,
        override_l1_data_gas_price_fri: null,
        override_l1_gas_price_fri: null,
        override_l2_gas_price_fri: null,
      },
    },
    network_config: {
      advertised_multiaddr: null,
      bootstrap_peer_multiaddr: null,
      port: 53080,
    },
    staking_manager_config: {
      dynamic_config: {
        default_committee: '0,100:',
        override_committee: null,
      },
    },
  },
  gateway_config: {
    static_config: {
      authorized_declarer_accounts: null,
      proof_archive_writer_config: {
        bucket_name: 'test-bucket',
      },
      stateful_tx_validator_config: {
        max_allowed_nonce_gap: 200,
      },
      stateless_tx_validator_config: {
        max_contract_bytecode_size: 81920,
        min_gas_price: 8000000000,
      },
    },
  },
  http_server_config: {
    static_config: {
      port: 8080,
    },
  },
  mempool_config: {
    dynamic_config: {
      transaction_ttl: 300,
    },
  },
  mempool_p2p_config: {
    network_config: {
      advertised_multiaddr: null,
      bootstrap_peer_multiaddr: null,
      port: 53200,
    },
  },
  monitoring_endpoint_config: {
    port: 8082,
  },
  sierra_compiler_config: {
    audited_libfuncs_only: false,
    max_bytecode_size: 81920,
  },
  state_sync_config: {
    static_config: {
      central_sync_client_config: {
        central_source_config: {
          class_cache_size: 128,
          concurrent_requests: 20,
          http_headers: '',
          max_classes_to_download: 20,
          max_state_updates_to_download: 20,
          max_state_updates_to_store_in_memory: 20,
          retry_config: {
            max_retries: 10,
            retry_base_millis: 30,
            retry_max_delay_millis: 30000,
          },
          starknet_url: 'https://starknet_url/',
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
      },
      network_config: null,
      p2p_sync_client_config: null,
      rpc_config: {
        port: 8083,
      },
    },
  },
}
