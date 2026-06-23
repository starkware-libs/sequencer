// Overrides layer for the `sepolia-integration` environment (hybrid layout).
{
  chain_id: 'SN_INTEGRATION_SEPOLIA',
  native_classes_whitelist: 'All',
  recorder_url: 'http://starknet-sepolia-integration.cende-recorder-proxy.starknet.io/',
  starknet_url: 'https://feeder.integration-sepolia.starknet.io/',

  base_layer_config: {
    bpo1_start_block_number: 9456501,
    bpo2_start_block_number: 9504747,
    fusaka_no_bpo_start_block_number: 9408577,
    starknet_contract_address: '0x4737c0c1B4D5b1A687B42610DdabEE781152359c',
  },

  batcher_config: {
    dynamic_config: {
      n_concurrent_txs: 2,
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
      first_block_with_partial_block_hash: {
        block_hash: '0x1ea2a9cfa3df5297d58c0a04d09d276bc68d40fe64701305bbe2ed8f417e869',
        block_number: 35748,
        parent_block_hash: '0x77140bef51bbb4d1932f17cc5081825ff18465a1df4440ca0429a4fa80f1dc5',
      },
    },
  },

  committer_config: {
    storage_config: {
      cache_size: 10000000,
    },
  },

  consensus_manager_config: {
    context_config: {
      dynamic_config: {
        min_l2_gas_price_per_height: '',
      },
    },
    staking_manager_config: {
      dynamic_config: {
        default_committee: '0,10:0x64,1,0x1,true;0x65,1,0x1,true;0x66,1,0x1,true',
      },
    },
  },

  gateway_config: {
    static_config: {
      proof_archive_writer_config: {
        bucket_name: 'starkware-starknet-integration',
      },
    },
  },

  sierra_compiler_config: {
    audited_libfuncs_only: false,
  },

  state_sync_config: {
    static_config: {
      central_sync_client_config: {
        sync_config: {
          store_sierras_and_casms_block_threshold: 0,
        },
      },
      network_config: null,
    },
  },
}
