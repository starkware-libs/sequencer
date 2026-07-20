// chain_params for `mainnet` environment.
// The env-shared P2P multiaddrs and per-node validator_id are in the devops repo.
local constants = import 'lib/base_layer_constants.libsonnet';
{
  mandatory: {
    chain_id: 'SN_MAIN',
    starknet_url: 'https://feeder.alpha-mainnet.starknet.io/',
    recorder_url: 'http://starknet-mainnet.cende-recorder-proxy.starknet.io/',
    starknet_contract_address: '0xc662c410C0ECf747543f5bA90660f6ABeBD9C8c4',
    base_layer: constants.ETH_MAINNET_BASE_LAYER,
    staking_default_committee: '0,10:0x64,1,0x1,true;0x65,1,0x1,true;0x66,1,0x1,true;0x67,1,0x1,true;0x68,1,0x1,true',
    proof_archive_bucket_name: 'starkware-starknet-mainnet',
    nodes_at_same_cluster: false,
    topology: import 'lib/layouts/hybrid.libsonnet',
  },

  native_classes_whitelist: '["0x054c5afe61ed27be53b1e4dec5707209a9fcabdb14712fb800fbc60439090115"]',
  max_state_diff_in_block: 5000,
  n_execution_workers: 12,
  first_block_with_partial_block_hash: {
    block_hash: '0x12889b177c93baa28b5ee3afc80cb6f4836adac086af4bef25ae1ac762e8a62',
    block_number: 671813,
    parent_block_hash: '0x1e68b0d22b14688dc97afa3006a53cf4e62ebcb02102e80f55e8b48f9a28b97',
  },
  committer_cache_size: 50000000,
  min_l2_gas_price_per_height: '8269292:27400000000,8742344:30100000000',
  central_sync_client_config: {
    sync_config: {
      store_sierras_and_casms_block_threshold: 103129,
    },
  },
  state_sync_network_config: {
    port: 55010,
  },
}
