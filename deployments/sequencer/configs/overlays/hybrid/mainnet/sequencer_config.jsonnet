// Overrides layer for the `mainnet` environment (hybrid layout).
{
  chain_id: 'SN_MAIN',
  native_classes_whitelist: '["0x054c5afe61ed27be53b1e4dec5707209a9fcabdb14712fb800fbc60439090115"]',
  recorder_url: 'http://starknet-mainnet.cende-recorder-proxy.starknet.io/',
  starknet_url: 'https://feeder.alpha-mainnet.starknet.io/',

  base_layer_config: {
    bpo1_start_block_number: 23973546,
    bpo2_start_block_number: 24168146,
    fusaka_no_bpo_start_block_number: 23934586,
    starknet_contract_address: '0xc662c410C0ECf747543f5bA90660f6ABeBD9C8c4',
  },

  batcher_config: {
    dynamic_config: {
      n_concurrent_txs: 100,
    },
    static_config: {
      block_builder_config: {
        bouncer_config: {
          block_max_capacity: {
            state_diff_size: 5000,
          },
        },
        execute_config: {
          n_workers: 12,
        },
      },
      first_block_with_partial_block_hash: {
        block_hash: '0x12889b177c93baa28b5ee3afc80cb6f4836adac086af4bef25ae1ac762e8a62',
        block_number: 671813,
        parent_block_hash: '0x1e68b0d22b14688dc97afa3006a53cf4e62ebcb02102e80f55e8b48f9a28b97',
      },
    },
  },

  committer_config: {
    storage_config: {
      cache_size: 50000000,
    },
  },

  consensus_manager_config: {
    context_config: {
      dynamic_config: {
        min_l2_gas_price_per_height: '8269292:27400000000,8742344:30100000000',
      },
    },
    staking_manager_config: {
      dynamic_config: {
        default_committee: '0,10:0x64,1,0x1,true;0x65,1,0x1,true;0x66,1,0x1,true;0x67,1,0x1,true;0x68,1,0x1,true',
      },
    },
  },

  gateway_config: {
    static_config: {
      proof_archive_writer_config: {
        bucket_name: 'starkware-starknet-mainnet',
      },
    },
  },

  sierra_compiler_config: {
    audited_libfuncs_only: true,
  },

  state_sync_config: {
    static_config: {
      central_sync_client_config: {
        sync_config: {
          store_sierras_and_casms_block_threshold: 103129,
        },
      },
      network_config: {
        port: 55010,
      },
    },
  },
}
