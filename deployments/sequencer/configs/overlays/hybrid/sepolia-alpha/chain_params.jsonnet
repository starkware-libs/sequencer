// chain_params for the `sepolia-alpha` environment (hybrid layout): the mandatory per-chain values,
// read directly by the applicative config. (The env-shared P2P multiaddrs and per-node validator_id
// are supplied by the devops overlay layers.)
local constants = import 'lib/constants.libsonnet';
{
  chain_id: 'SN_SEPOLIA',
  starknet_url: 'https://feeder.alpha-sepolia.starknet.io/',
  recorder_url: 'http://starknet-sepolia-alpha.cende-recorder-proxy.starknet.io/',
  native_classes_whitelist: 'All',
  starknet_contract_address: '0xE2Bb56ee936fd6433DC0F6e7e3b8365C906AA057',
  base_layer_config: constants.ETH_TESTNET_BASE_LAYER,
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
}
