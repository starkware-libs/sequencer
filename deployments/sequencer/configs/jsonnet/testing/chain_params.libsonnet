// Testing chain_params: the per chain/env mandatory values, read directly by the applicative config.
{
  mandatory: {
    chain_id: 'SN_SEPOLIA',
    recorder_url: 'https://recorder_url',
    starknet_url: 'https://starknet_url/',
    starknet_contract_address: '0x0000000000000000000000000000000000000001',
    base_layer: {
      bpo1_start_block_number: 9456501,
      bpo2_start_block_number: 9504747,
      fusaka_no_bpo_start_block_number: 9408577,
    },
    staking_default_committee: '0,100:',
    proof_archive_bucket_name: 'test-bucket',
    nodes_at_same_cluster: true,
  },
  consensus_bootstrap_peer_multiaddr: null,
  mempool_bootstrap_peer_multiaddr: null,
}
