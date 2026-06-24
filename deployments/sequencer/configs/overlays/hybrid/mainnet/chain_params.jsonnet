// chain_params for the `mainnet` environment (hybrid layout): the mandatory per-chain values, read
// directly by the applicative config. (The env-shared P2P multiaddrs and per-node validator_id are
// supplied by the devops overlay layers.)
{
  chain_id: 'SN_MAIN',
  starknet_url: 'https://feeder.alpha-mainnet.starknet.io/',
  recorder_url: 'http://starknet-mainnet.cende-recorder-proxy.starknet.io/',
  starknet_contract_address: '0xc662c410C0ECf747543f5bA90660f6ABeBD9C8c4',
  base_layer_config: {
    bpo1_start_block_number: 23973546,
    bpo2_start_block_number: 24168146,
    fusaka_no_bpo_start_block_number: 23934586,
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
        bucket_name: 'starkware-starknet-mainnet',
      },
    },
  },
}
