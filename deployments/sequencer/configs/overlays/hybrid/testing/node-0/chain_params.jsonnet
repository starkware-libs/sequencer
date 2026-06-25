// chain_params for the `testing/node-0` overlay (hybrid layout): the mandatory per-chain values,
// read directly by the applicative config. Functional overlay deployed by `hybrid_system_test.yaml`.
{
  chain_id: 'CHAIN_ID_SUBDIR',
  starknet_url: 'https://integration-sepolia.starknet.io/',
  recorder_url: 'http://dummy-recorder-service.dummy-recorder.svc.cluster.local:8080',
  native_classes_whitelist: 'All',
  base_layer_config: {
    bpo1_start_block_number: 13205504,
    bpo2_start_block_number: 13410304,
    fusaka_no_bpo_start_block_number: 13164544,
    starknet_contract_address: '0x5FbDB2315678afecb367f032d93F642f64180aa3',
  },
  consensus_manager_config: {
    staking_manager_config: {
      dynamic_config: {
        default_committee: '0,100:0x64,1,0x1,true',
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
