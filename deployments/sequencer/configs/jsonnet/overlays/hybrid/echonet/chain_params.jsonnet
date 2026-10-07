// chain_params for the echonet `hybrid` overlay: a mainnet replay node whose upstream feeder, recorder
// and L1 are all served by the echonet pod.
local constants = import 'lib/base_layer_constants.libsonnet';
{
  mandatory: {
    chain_id: 'SN_MAIN',
    starknet_url: 'http://echonet:80',
    recorder_url: 'http://echonet:80',
    // Echonet's fake L1 is an anvil-style chain, not Ethereum mainnet.
    starknet_contract_address: '0x5FbDB2315678afecb367f032d93F642f64180aa3',
    base_layer: constants.ETH_TESTNET_BASE_LAYER,
    staking_default_committee: {
      start_epoch: 0,
      committee_size: 100,
      stakers: [
        { address: '0x64', weight: 1, public_key: '0x1', can_propose: true },
      ],
    },
    proof_archive_bucket_name: '',
    nodes_at_same_cluster: true,
    topology: import 'lib/layouts/hybrid.libsonnet',
  },
  consensus_bootstrap_peer_multiaddr: null,
  mempool_bootstrap_peer_multiaddr: null,

  // Overrides of the applicative-config defaults; each falls back to `default_replacers` when absent.
  eth_fee_token_address: '0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7',
  strk_fee_token_address: '0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d',
  native_classes_whitelist: 'All',
  n_concurrent_txs: 100,
  proposer_idle_detection_delay_millis: 2000,
  max_events_in_block: 5000,
  max_receipt_l2_gas_in_block: 5800000000,
  max_state_diff_in_block: 4000,
  n_execution_workers: 28,
  first_block_with_partial_block_hash: {
    block_hash: '0x12889b177c93baa28b5ee3afc80cb6f4836adac086af4bef25ae1ac762e8a62',
    block_number: 671813,
    parent_block_hash: '0x1e68b0d22b14688dc97afa3006a53cf4e62ebcb02102e80f55e8b48f9a28b97',
  },
  committer_cache_size: 10000000,
  committer_inner_storage_cache_size: 8589934592,
  proposal_timeout_base: 9.1,
  proposal_timeout_max: 15.0,
  min_l2_gas_price_per_height: [{ height: 6832000, price: 27400000000 }],
  compare_retrospective_block_hash: false,
  authorized_declarer_accounts: null,
  max_allowed_nonce_gap: 200,
  max_contract_bytecode_size: 81920,
  min_gas_price: 3000000000,
  transaction_ttl: 300,
  audited_libfuncs_only: false,
  max_bytecode_size: 4089446,
  central_sync_client_config: {},
  state_sync_network_config: { port: 55010 },
  p2p_sync_client_config: null,
}
