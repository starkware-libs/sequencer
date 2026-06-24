// chain_params for the `sepolia-alpha` environment (hybrid layout): the mandatory per-chain values,
// read directly by the applicative config. (The env-shared P2P multiaddrs and per-node validator_id
// are supplied by the devops overlay layers.)
{
  chain_id: 'SN_SEPOLIA',
  starknet_url: 'https://feeder.alpha-sepolia.starknet.io/',
  recorder_url: 'http://starknet-sepolia-alpha.cende-recorder-proxy.starknet.io/',
  native_classes_whitelist: 'All',
  base_layer_config: {
    bpo1_start_block_number: 9456501,
    bpo2_start_block_number: 9504747,
    fusaka_no_bpo_start_block_number: 9408577,
    starknet_contract_address: '0xE2Bb56ee936fd6433DC0F6e7e3b8365C906AA057',
  },
  consensus_manager_config: {
    staking_manager_config: {
      dynamic_config: {
        default_committee: '0,10:0x64,1,0x1,true;0x65,1,0x1,true;0x66,1,0x1,true;0x67,1,0x1,true;0x68,1,0x1,true',
      },
    },
  },
  gateway_config: {
    static_config: {
      proof_archive_writer_config: {
        bucket_name: 'starkware-starknet-alpha',
      },
    },
  },
  batcher_config: {
    static_config: {
      first_block_with_partial_block_hash: {
        block_hash: '0x578b4e2f34e4da24e7482de643b4e3435fa7e34770cdb8d71002bb19e415ffa',
        block_number: 86311,
        parent_block_hash: '0x5c980ea7747167d2ae98fa7ef7d62f52243e924c453b4934045443d977458d3',
      },
    },
  },
}
