// chain_params for `sepolia-integration` environment.
// The env-shared P2P multiaddrs and per-node validator_id are in the devops repo.
local constants = import 'lib/constants.libsonnet';
{
  chain_id: 'SN_INTEGRATION_SEPOLIA',
  starknet_url: 'https://feeder.integration-sepolia.starknet.io/',
  recorder_url: 'http://starknet-sepolia-integration.cende-recorder-proxy.starknet.io/',
  starknet_contract_address: '0x4737c0c1B4D5b1A687B42610DdabEE781152359c',
  base_layer: constants.ETH_TESTNET_BASE_LAYER,
  staking_default_committee: '0,10:0x64,1,0x1,true;0x65,1,0x1,true;0x66,1,0x1,true',
  proof_archive_bucket_name: 'starkware-starknet-integration',

  native_classes_whitelist: 'All',
  n_concurrent_txs: 2,
  n_execution_workers: 1,
  first_block_with_partial_block_hash: {
    block_hash: '0x1ea2a9cfa3df5297d58c0a04d09d276bc68d40fe64701305bbe2ed8f417e869',
    block_number: 35748,
    parent_block_hash: '0x77140bef51bbb4d1932f17cc5081825ff18465a1df4440ca0429a4fa80f1dc5',
  },
  audited_libfuncs_only: false,
}
