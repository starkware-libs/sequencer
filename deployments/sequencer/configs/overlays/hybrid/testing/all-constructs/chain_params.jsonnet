// chain_params for the `testing/all-constructs` overlay (hybrid layout): clearly-dummy per-chain
// values, the minimum for native synth to succeed. NONE of these are deployed anywhere; the synth
// this overlay feeds is only `kubectl validate`d for manifest structure, never for config content.
{
  chain_id: 'SN_DUMMY',
  starknet_url: 'http://dummy-starknet/',
  recorder_url: 'http://dummy-recorder/',
  native_classes_whitelist: 'All',
  base_layer_config: {
    bpo1_start_block_number: 0,
    bpo2_start_block_number: 0,
    fusaka_no_bpo_start_block_number: 0,
    starknet_contract_address: '0x0',
  },
  consensus_manager_config: {
    staking_manager_config: {
      dynamic_config: {
        default_committee: '0,10:0x64,1,0x1,true',
      },
    },
    network_config: {
      advertised_multiaddr: null,
      bootstrap_peer_multiaddr: null,
    },
  },
  mempool_p2p_config: {
    network_config: {
      advertised_multiaddr: null,
      bootstrap_peer_multiaddr: null,
    },
  },
  gateway_config: {
    static_config: {
      proof_archive_writer_config: {
        bucket_name: '',
      },
    },
  },
  batcher_config: {
    static_config: {
      first_block_with_partial_block_hash: null,
    },
  },
}
