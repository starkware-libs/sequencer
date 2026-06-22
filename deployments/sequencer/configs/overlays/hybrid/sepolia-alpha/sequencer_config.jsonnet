// IN-PLACE nested-overrides for the hybrid `sepolia-alpha` overlay layer.
//
// Self-contained: holds only this layer's own `config.sequencerConfig` deltas, aggregated from
// `common.yaml` and `services/*.yaml` and converted to nested, `#is_none`-folded form. Deep-merged
// over the `common` layer at deploy order. No k8s scaffolding and no `components.*` keys.
{
  chain_id: 'SN_SEPOLIA',
  native_classes_whitelist: 'All',
  recorder_url: 'http://starknet-sepolia-alpha.cende-recorder-proxy.starknet.io/',
  starknet_url: 'https://feeder.alpha-sepolia.starknet.io/',
  committer_config: {
    storage_config: {
      cache_size: 10000000,
      inner_storage_config: {
        cache_size: 1073741824,
      },
    },
  },
  batcher_config: {
    dynamic_config: {
      n_concurrent_txs: 8,
    },
    static_config: {
      block_builder_config: {
        bouncer_config: {
          block_max_capacity: {
            state_diff_size: 5000,
          },
        },
        execute_config: {
          n_workers: 5,
        },
      },
      // `#is_none: false` with materialized children -> the children survive (`Some(value)`).
      first_block_with_partial_block_hash: {
        block_hash: '0x578b4e2f34e4da24e7482de643b4e3435fa7e34770cdb8d71002bb19e415ffa',
        block_number: 86311,
        parent_block_hash: '0x5c980ea7747167d2ae98fa7ef7d62f52243e924c453b4934045443d977458d3',
      },
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
        default_committee: '0,10:0x64,1,0x1,true;0x65,1,0x1,true;0x66,1,0x1,true;0x67,1,0x1,true;0x68,1,0x1,true',
      },
    },
  },
  state_sync_config: {
    static_config: {
      // `#is_none: false` with children -> survives as `Some`.
      central_sync_client_config: {
        sync_config: {
          store_sierras_and_casms_block_threshold: 0,
        },
      },
      // `#is_none: true` -> folds to null; placeholder `port` is dropped.
      network_config: null,
    },
  },
  gateway_config: {
    static_config: {
      proof_archive_writer_config: {
        bucket_name: 'starkware-starknet-alpha',
      },
    },
  },
  base_layer_config: {
    bpo1_start_block_number: 9456501,
    bpo2_start_block_number: 9504747,
    fusaka_no_bpo_start_block_number: 9408577,
    starknet_contract_address: '0xE2Bb56ee936fd6433DC0F6e7e3b8365C906AA057',
  },
  sierra_compiler_config: {
    audited_libfuncs_only: true,
  },
}
