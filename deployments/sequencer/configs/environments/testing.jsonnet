// Environment-specific configuration for the hybrid testing environment.
// Imports the hybrid layout (which has mustOverride markers for env-specific fields)
// and provides concrete values for all mustOverride fields.

local layout = import "../../../../crates/apollo_deployments/jsonnet/layouts/hybrid.jsonnet";

local chain_id = "CHAIN_ID_SUBDIR";
local eth_fee = "0x1001";
local strk_fee = "0x1002";
local recorder_url = "http://dummy-recorder-service.dummy-recorder.svc.cluster.local:8080";
local starknet_url = "https://integration-sepolia.starknet.io/";
local validator_id = "0x64";
local monitoring_port = 8082;

// Full NetworkConfig struct for state_sync (which holds it as Option<NetworkConfig>).
// Provided as a complete struct since the field itself is a whole-struct mustOverride.
local stateSyncNetworkConfig(port) = {
  advertised_multiaddr: null,
  bootstrap_peer_multiaddr: null,
  broadcasted_message_metadata_buffer_size: 100000,
  chain_id: chain_id,
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
  port: port,
  prune_dead_connections_ping_interval: 15,
  prune_dead_connections_ping_timeout: 20,
  reported_peer_ids_buffer_size: 100000,
  secret_key: "",
  session_timeout: 120,
};

// Default P2P sync client config (Option<P2pSyncClientConfig> = Some with defaults).
local p2pSyncClientConfig = {
  buffer_size: 100000,
  num_block_classes_per_query: 100,
  num_block_state_diffs_per_query: 100,
  num_block_transactions_per_query: 100,
  num_headers_per_query: 10000,
  wait_period_for_new_data: 50,
  wait_period_for_other_protocol: 50,
};

// Versioned constants overrides (Option<VersionedConstantsOverrides> = Some).
local versionedConstantsOverrides = {
  invoke_tx_max_n_steps: 10000000,
  max_n_events: 1000,
  max_recursion_depth: 50,
  validate_max_n_steps: 1000000,
};

local node0 = {
  committer: layout.committer {
    committer_config+: {
      storage_config+: {
        cache_size: 1000000,
      },
      verify_state_diff_hash: true,
    },
    monitoring_endpoint_config+: {
      port: monitoring_port,
    },
    components+: {
      batcher+: { port: 55000, url: "sequencer-core-service" },
      committer+: { port: 55013, url: "sequencer-committer-service" },
    },
  },

  core: layout.core {
    batcher_config+: {
      dynamic_config+: {
        n_concurrent_txs: 100,
        native_classes_whitelist: "All",
        proposer_idle_detection_delay_millis: 2000,
      },
      static_config+: {
        block_builder_config+: {
          bouncer_config+: {
            block_max_capacity+: {
              n_events: 5000,
              receipt_l2_gas: 5800000000,
              state_diff_size: 4000,
            },
          },
          chain_info+: {
            chain_id: chain_id,
            fee_token_addresses+: {
              eth_fee_token_address: eth_fee,
              strk_fee_token_address: strk_fee,
            },
          },
          execute_config+: {
            n_workers: 28,
          },
          versioned_constants_overrides: versionedConstantsOverrides,
        },
        contract_class_manager_config+: {
          native_compiler_config+: {
            max_cpu_time: 600,
          },
        },
        first_block_with_partial_block_hash: null,
        pre_confirmed_cende_config+: {
          recorder_url: recorder_url,
        },
        storage+: {
          db_config+: {
            chain_id: chain_id,
          },
        },
        storage_reader_server_static_config+: {
          port: 55011,
        },
        validation_only: false,
      },
    },
    class_manager_config+: {
      static_config+: {
        class_manager_config+: {
          max_compiled_contract_class_object_size: 4089446,
        },
        class_storage_config+: {
          class_hash_storage_config+: {
            db_config+: {
              chain_id: chain_id,
            },
          },
          storage_reader_server_static_config+: {
            port: 55210,
          },
        },
      },
    },
    consensus_manager_config+: {
      cende_config+: {
        recorder_url: recorder_url,
      },
      consensus_manager_config+: {
        dynamic_config+: {
          require_virtual_proposer_vote: false,
          timeouts+: {
            proposal+: {
              base: 9.1,
              max: 15.0,
            },
          },
          validator_id: validator_id,
        },
        static_config+: {
          storage_config+: {
            db_config+: {
              chain_id: chain_id,
            },
          },
        },
      },
      context_config+: {
        dynamic_config+: {
          build_proposal_margin_millis: 1000,
          compare_retrospective_block_hash: false,
          min_l2_gas_price_per_height: "",
          override_eth_to_fri_rate: null,
          override_l1_data_gas_price_fri: null,
          override_l1_gas_price_fri: null,
          override_l2_gas_price_fri: null,
        },
        static_config+: {
          behavior_mode: "starknet",
          chain_id: chain_id,
        },
      },
      network_config+: {
        advertised_multiaddr: null,
        bootstrap_peer_multiaddr: null,
        chain_id: chain_id,
        port: 53080,
      },
      revert_config+: {
        revert_up_to_and_including: 0,
        should_revert: false,
      },
      staking_manager_config+: {
        dynamic_config+: {
          default_committee: "0,100:0x64,1,0x1,true",
          override_committee: null,
        },
      },
    },
    monitoring_endpoint_config+: {
      port: monitoring_port,
    },
    state_sync_config+: {
      static_config+: {
        central_sync_client_config: null,
        network_config: stateSyncNetworkConfig(55010),
        p2p_sync_client_config: p2pSyncClientConfig,
        revert_config+: {
          revert_up_to_and_including: 0,
          should_revert: false,
        },
        rpc_config+: {
          chain_id: chain_id,
          execution_config+: {
            eth_fee_contract_address: eth_fee,
            strk_fee_contract_address: strk_fee,
          },
          port: 8090,
          starknet_url: starknet_url,
        },
        storage_config+: {
          db_config+: {
            chain_id: chain_id,
          },
        },
        storage_reader_server_static_config+: {
          port: 55014,
        },
      },
    },
    components+: {
      batcher+: { port: 55000, url: "sequencer-core-service" },
      class_manager+: { port: 55001, url: "sequencer-core-service" },
      committer+: { port: 55013, url: "sequencer-committer-service" },
      l1_events_provider+: { port: 55004, url: "sequencer-l1-service" },
      l1_gas_price_provider+: { port: 55003, url: "sequencer-l1-service" },
      mempool+: { port: 55006, url: "sequencer-mempool-service" },
      proof_manager+: { port: 55012, url: "sequencer-core-service" },
      sierra_compiler+: { port: 55007, url: "sequencer-sierracompiler-service" },
      signature_manager+: { port: 55008, url: "sequencer-core-service" },
      state_sync+: { port: 55009, url: "sequencer-core-service" },
    },
  },

  gateway: layout.gateway {
    gateway_config+: {
      dynamic_config+: {
        native_classes_whitelist: "All",
      },
      static_config+: {
        authorized_declarer_accounts: null,
        chain_info+: {
          chain_id: chain_id,
          fee_token_addresses+: {
            eth_fee_token_address: eth_fee,
            strk_fee_token_address: strk_fee,
          },
        },
        contract_class_manager_config+: {
          native_compiler_config+: {
            max_cpu_time: 600,
          },
        },
        proof_archive_writer_config+: {
          bucket_name: "",
        },
        stateful_tx_validator_config+: {
          max_allowed_nonce_gap: 200,
          validate_resource_bounds: true,
          versioned_constants_overrides: null,
        },
        stateless_tx_validator_config+: {
          max_contract_bytecode_size: 81920,
          min_gas_price: 3000000000,
          validate_resource_bounds: true,
        },
      },
    },
    http_server_config+: {
      static_config+: {
        port: 8080,
      },
    },
    monitoring_endpoint_config+: {
      port: monitoring_port,
    },
    components+: {
      class_manager+: { port: 55001, url: "sequencer-core-service" },
      gateway+: { port: 55002, url: "sequencer-gateway-service" },
      mempool+: { port: 55006, url: "sequencer-mempool-service" },
      proof_manager+: { port: 55012, url: "sequencer-core-service" },
      state_sync+: { port: 55009, url: "sequencer-core-service" },
    },
  },

  l1: layout.l1 {
    base_layer_config+: {
      bpo1_start_block_number: 13205504,
      bpo2_start_block_number: 13410304,
      fusaka_no_bpo_start_block_number: 13164544,
      starknet_contract_address: "0x5FbDB2315678afecb367f032d93F642f64180aa3",
    },
    l1_events_scraper_config+: {
      chain_id: chain_id,
    },
    l1_gas_price_scraper_config+: {
      chain_id: chain_id,
    },
    monitoring_endpoint_config+: {
      port: monitoring_port,
    },
    components+: {
      batcher+: { port: 55000, url: "sequencer-core-service" },
      l1_events_provider+: { port: 55004, url: "sequencer-l1-service" },
      l1_gas_price_provider+: { port: 55003, url: "sequencer-l1-service" },
      state_sync+: { port: 55009, url: "sequencer-core-service" },
    },
  },

  mempool: layout.mempool {
    mempool_config+: {
      dynamic_config+: {
        transaction_ttl: 300,
      },
      static_config+: {
        behavior_mode: "starknet",
        recorder_url: recorder_url,
        validate_resource_bounds: true,
      },
    },
    mempool_p2p_config+: {
      network_config+: {
        advertised_multiaddr: null,
        bootstrap_peer_multiaddr: null,
        chain_id: chain_id,
        port: 53200,
      },
    },
    monitoring_endpoint_config+: {
      port: monitoring_port,
    },
    components+: {
      class_manager+: { port: 55001, url: "sequencer-core-service" },
      gateway+: { port: 55002, url: "sequencer-gateway-service" },
      mempool+: { port: 55006, url: "sequencer-mempool-service" },
      proof_manager+: { port: 55012, url: "sequencer-core-service" },
    },
  },

  sierra_compiler: layout.sierra_compiler {
    sierra_compiler_config+: {
      audited_libfuncs_only: false,
      max_bytecode_size: 81920,
      max_cpu_time: 60,
    },
    monitoring_endpoint_config+: {
      port: monitoring_port,
    },
    components+: {
      sierra_compiler+: { port: 55007, url: "sequencer-sierracompiler-service" },
    },
  },
};

{
  "node-0": node0,
  "all-constructs": node0,
}
