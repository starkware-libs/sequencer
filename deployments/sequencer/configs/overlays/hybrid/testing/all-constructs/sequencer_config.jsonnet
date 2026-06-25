// Overrides layer for the `testing/all-constructs` overlay (hybrid layout).
//
// `all-constructs` is a STRUCTURE-validation stub: the cdk8s synth it feeds is only `kubectl
// validate`d for manifest structure, never for config content. Its YAML `config.sequencerConfig`
// carries no real applicative values (only `components.fake_field`, which folds away). Native synth,
// however, hard-requires the per-environment `overrides.*` reads that `applicative_config.libsonnet`
// performs unconditionally and that the base `common` layer does not already supply (every service's
// component config is assembled, including the l1/batcher/state-sync paths). This layer supplies the
// minimum for `build()` to succeed with clearly-dummy values; NONE of these values are deployed
// anywhere, so they are immaterial.
//
// Because this layer intentionally carries fields its YAMLs lack, it is NOT subject to the strict
// per-layer YAML-mirror invariant the env/dummy layers obey; the regression test for it only asserts
// that native synth succeeds (see `test_all_constructs_native_config_synthesizes`).
{
  chain_id: 'SN_DUMMY',
  native_classes_whitelist: 'All',
  recorder_url: 'http://dummy-recorder/',
  starknet_url: 'http://dummy-starknet/',
  validator_id: '0x64',

  base_layer_config: {
    bpo1_start_block_number: 0,
    bpo2_start_block_number: 0,
    fusaka_no_bpo_start_block_number: 0,
    starknet_contract_address: '0x0',
  },

  batcher_config: {
    dynamic_config: {
      n_concurrent_txs: 1,
    },
    static_config: {
      block_builder_config: {
        bouncer_config: {
          block_max_capacity: {
            state_diff_size: 4000,
          },
        },
        execute_config: {
          n_workers: 1,
        },
      },
      first_block_with_partial_block_hash: null,
    },
  },

  committer_config: {
    storage_config: {
      cache_size: 1000000,
    },
  },

  consensus_manager_config: {
    context_config: {
      dynamic_config: {
        min_l2_gas_price_per_height: '',
      },
    },
    network_config: {
      advertised_multiaddr: null,
      bootstrap_peer_multiaddr: null,
    },
    staking_manager_config: {
      dynamic_config: {
        default_committee: '0,10:0x64,1,0x1,true',
      },
    },
  },

  gateway_config: {
    static_config: {
      proof_archive_writer_config: {
        bucket_name: '',
      },
    },
  },

  mempool_p2p_config: {
    network_config: {
      advertised_multiaddr: null,
      bootstrap_peer_multiaddr: null,
    },
  },

  sierra_compiler_config: {
    audited_libfuncs_only: false,
  },

  state_sync_config: {
    static_config: {
      central_sync_client_config: null,
      network_config: null,
    },
  },
}
