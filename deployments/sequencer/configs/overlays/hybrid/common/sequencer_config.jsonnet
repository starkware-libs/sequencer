// IN-PLACE nested-overrides for the hybrid `common` overlay layer.
//
// Self-contained: holds only this layer's own `config.sequencerConfig` deltas, aggregated from
// `common.yaml` and `services/*.yaml` and converted to nested, `#is_none`-folded form
// (`a.b.c: v` -> `{a:{b:{c:v}}}`; `<path>.#is_none: true` -> `<path>: null`). No k8s scaffolding
// and no `components.*` keys (layout-derived infra the generator computes, not overrides).
{
  eth_fee_token_address: '0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7',
  strk_fee_token_address: '0x04718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d',
  monitoring_endpoint_config: {
    port: 8082,
  },
  // `#is_none: true` -> the whole optional folds to null (its placeholder children are dropped).
  versioned_constants_overrides: null,
  committer_config: {
    verify_state_diff_hash: true,
    storage_config: {
      inner_storage_config: {
        cache_size: 8589934592,
      },
    },
  },
  batcher_config: {
    dynamic_config: {
      proposer_idle_detection_delay_millis: 2000,
    },
    static_config: {
      block_builder_config: {
        bouncer_config: {
          block_max_capacity: {
            n_events: 5000,
            receipt_l2_gas: 5800000000,
          },
        },
      },
      storage_reader_server_static_config: {
        port: 55011,
      },
    },
  },
  class_manager_config: {
    static_config: {
      class_manager_config: {
        max_compiled_contract_class_object_size: 4089446,
      },
      class_storage_config: {
        storage_reader_server_static_config: {
          port: 55210,
        },
      },
    },
  },
  consensus_manager_config: {
    consensus_manager_config: {
      dynamic_config: {
        require_virtual_proposer_vote: false,
        timeouts: {
          proposal: {
            base: 9.1,
            max: 9.1,
          },
        },
      },
    },
    context_config: {
      dynamic_config: {
        build_proposal_margin_millis: 1000,
        compare_retrospective_block_hash: true,
        override_eth_to_fri_rate: null,
        override_l1_data_gas_price_fri: null,
        override_l1_gas_price_fri: null,
        override_l2_gas_price_fri: null,
      },
    },
    network_config: {
      port: 53080,
    },
    staking_manager_config: {
      dynamic_config: {
        override_committee: null,
      },
    },
  },
  state_sync_config: {
    static_config: {
      p2p_sync_client_config: null,
      rpc_config: {
        port: 8090,
      },
      storage_reader_server_static_config: {
        port: 55014,
      },
    },
  },
  gateway_config: {
    static_config: {
      authorized_declarer_accounts: null,
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
      port: 53200,
    },
  },
  sierra_compiler_config: {
    max_bytecode_size: 81920,
  },
}
