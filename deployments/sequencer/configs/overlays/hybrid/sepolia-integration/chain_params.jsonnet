// chain_params for the `sepolia-integration` environment (hybrid layout): the mandatory per-chain
// values, read directly by the applicative config. (The env-shared P2P multiaddrs and per-node
// validator_id are supplied by the devops overlay layers.)
{
  chain_id: 'SN_INTEGRATION_SEPOLIA',
  starknet_url: 'https://feeder.integration-sepolia.starknet.io/',
  recorder_url: 'http://starknet-sepolia-integration.cende-recorder-proxy.starknet.io/',
  native_classes_whitelist: 'All',
  base_layer_config: {
    bpo1_start_block_number: 9456501,
    bpo2_start_block_number: 9504747,
    fusaka_no_bpo_start_block_number: 9408577,
    starknet_contract_address: '0x4737c0c1B4D5b1A687B42610DdabEE781152359c',
  },
  consensus_manager_config: {
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
  batcher_config: {
    static_config: {
      first_block_with_partial_block_hash: {
        block_hash: '0x1ea2a9cfa3df5297d58c0a04d09d276bc68d40fe64701305bbe2ed8f417e869',
        block_number: 35748,
        parent_block_hash: '0x77140bef51bbb4d1932f17cc5081825ff18465a1df4440ca0429a4fa80f1dc5',
      },
    },
  },
}
