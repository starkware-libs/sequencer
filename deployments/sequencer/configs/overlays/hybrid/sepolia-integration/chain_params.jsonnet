// chain_params for the `sepolia-integration` environment (hybrid layout): the mandatory per-chain
// values, read directly by the applicative config. (The env-shared P2P multiaddrs and per-node
// validator_id are supplied by the devops overlay layers.)
local constants = import 'lib/constants.libsonnet';
{
  chain_id: 'SN_INTEGRATION_SEPOLIA',
  starknet_url: 'https://feeder.integration-sepolia.starknet.io/',
  recorder_url: 'http://starknet-sepolia-integration.cende-recorder-proxy.starknet.io/',
  starknet_contract_address: '0x4737c0c1B4D5b1A687B42610DdabEE781152359c',
  base_layer_config: constants.ETH_TESTNET_BASE_LAYER,
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
}
