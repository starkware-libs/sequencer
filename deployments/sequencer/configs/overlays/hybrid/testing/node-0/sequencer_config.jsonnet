// Overrides layer for the `testing/node-0` overlay (hybrid layout).
// Full value-parity translation of the combined `config.sequencerConfig` across this overlay's
// `common.yaml` + `services/*.yaml` (folded: `#is_none` applied, `components.*` dropped). This is a
// FUNCTIONAL overlay deployed live by `hybrid_system_test.yaml`, so the values are load-bearing and
// must stay byte-for-byte aligned with the YAMLs (enforced by `test_native_config.py`).
{
  chain_id: 'CHAIN_ID_SUBDIR',
  eth_fee_token_address: '0x1001',
  native_classes_whitelist: 'All',
  recorder_url: 'http://dummy-recorder-service.dummy-recorder.svc.cluster.local:8080',
  starknet_url: 'https://integration-sepolia.starknet.io/',
  strk_fee_token_address: '0x1002',
  validator_id: '0x64',
  versioned_constants_overrides: null,

  base_layer_config: {
    bpo1_start_block_number: 13205504,
    bpo2_start_block_number: 13410304,
    fusaka_no_bpo_start_block_number: 13164544,
    starknet_contract_address: '0x5FbDB2315678afecb367f032d93F642f64180aa3',
  },

  batcher_config: {
    dynamic_config: {
      n_concurrent_txs: 100,
      proposer_idle_detection_delay_millis: 2000,
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
      cache_size: 1000000,
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
        default_committee: '0,100:0x64,1,0x1,true',
        override_committee: null,
      },
    },
  },

  gateway_config: {
    static_config: {
      authorized_declarer_accounts: null,
      proof_archive_writer_config: {
        bucket_name: '',
      },
      stateful_tx_validator_config: {
        max_allowed_nonce_gap: 200,
      },
      stateless_tx_validator_config: {
        max_contract_bytecode_size: 81920,
        min_gas_price: 3000000000,
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
      central_sync_client_config: null,
      network_config: {
        port: 55010,
      },
      rpc_config: {
        port: 8090,
      },
    },
  },
}
