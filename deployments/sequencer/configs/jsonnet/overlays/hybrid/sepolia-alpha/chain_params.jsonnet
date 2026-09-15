// chain_params for `sepolia-alpha` environment.
// The env-shared P2P multiaddrs and per-node validator_id are in the devops repo.
local constants = import 'lib/base_layer_constants.libsonnet';
{
  mandatory: {
    chain_id: 'SN_SEPOLIA',
    starknet_url: 'https://feeder.alpha-sepolia.starknet.io/',
    recorder_url: 'http://starknet-sepolia-alpha.cende-recorder-proxy.starknet.io/',
    starknet_contract_address: '0xE2Bb56ee936fd6433DC0F6e7e3b8365C906AA057',
    base_layer: constants.ETH_TESTNET_BASE_LAYER,
    staking_default_committee: {
      start_epoch: 0,
      committee_size: 10,
      stakers: [
        { address: '0x64', weight: 1, public_key: '0x1', can_propose: true },
        { address: '0x65', weight: 1, public_key: '0x1', can_propose: true },
        { address: '0x66', weight: 1, public_key: '0x1', can_propose: true },
        { address: '0x67', weight: 1, public_key: '0x1', can_propose: true },
        { address: '0x68', weight: 1, public_key: '0x1', can_propose: true },
      ],
    },
    proof_archive_bucket_name: 'starkware-starknet-alpha',
    nodes_at_same_cluster: false,
    topology: import 'lib/layouts/hybrid.libsonnet',
  },

  native_classes_whitelist: 'All',
  n_concurrent_txs: 8,
  max_state_diff_in_block: 5000,
  first_block_with_partial_block_hash: {
    block_hash: '0x578b4e2f34e4da24e7482de643b4e3435fa7e34770cdb8d71002bb19e415ffa',
    block_number: 86311,
    parent_block_hash: '0x5c980ea7747167d2ae98fa7ef7d62f52243e924c453b4934045443d977458d3',
  },
  committer_inner_storage_cache_size: 1073741824,
}
