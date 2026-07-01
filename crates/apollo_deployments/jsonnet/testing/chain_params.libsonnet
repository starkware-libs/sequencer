// Testing chain_params: the per chain/env mandatory values, read directly by the applicative config.
{
  chain_id: 'SN_SEPOLIA',
  native_classes_whitelist: '[]',
  recorder_url: 'https://recorder_url',
  starknet_url: 'https://starknet_url/',
  base_layer_config: {
    bpo1_start_block_number: 9456501,
    bpo2_start_block_number: 9504747,
    fusaka_no_bpo_start_block_number: 9408577,
    starknet_contract_address: '0x0000000000000000000000000000000000000001',
  },
  batcher_config: {
    static_config: {
      first_block_with_partial_block_hash: null,
    },
  },
  consensus_manager_config: {
    network_config: {
      advertised_multiaddr: null,
      bootstrap_peer_multiaddr: null,
    },
    staking_manager_config: {
      dynamic_config: {
        default_committee: '0,100:',
      },
    },
  },
  gateway_config: {
    static_config: {
      proof_archive_writer_config: {
        bucket_name: 'test-bucket',
      },
    },
  },
  mempool_p2p_config: {
    network_config: {
      advertised_multiaddr: null,
      bootstrap_peer_multiaddr: null,
    },
  },
}
