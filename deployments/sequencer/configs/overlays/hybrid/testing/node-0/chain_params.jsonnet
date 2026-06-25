// chain_params for the `testing/node-0` overlay (hybrid layout): the mandatory per-chain values plus
// this overlay's overrides of the applicative-config defaults (the latter fall back to
// `default_replacers`). Read directly by the applicative config. Functional overlay deployed by
// `hybrid_system_test.yaml`.
{
  mandatory: {
    chain_id: 'CHAIN_ID_SUBDIR',
    starknet_url: 'https://integration-sepolia.starknet.io/',
    recorder_url: 'http://dummy-recorder-service.dummy-recorder.svc.cluster.local:8080',
    starknet_contract_address: '0x5FbDB2315678afecb367f032d93F642f64180aa3',
    base_layer: {
      bpo1_start_block_number: 13205504,
      bpo2_start_block_number: 13410304,
      fusaka_no_bpo_start_block_number: 13164544,
    },
    staking_default_committee: '0,100:0x64,1,0x1,true',
    proof_archive_bucket_name: '',
    nodes_at_same_cluster: false,
    topology: import 'lib/layouts/hybrid.libsonnet',
  },
  consensus_bootstrap_peer_multiaddr: null,
  mempool_bootstrap_peer_multiaddr: null,

  // Overrides of the applicative-config defaults; each falls back to `default_replacers` when absent.
  native_classes_whitelist: 'All',
  eth_fee_token_address: '0x1001',
  strk_fee_token_address: '0x1002',
  proposer_idle_detection_delay_millis: 2000,
  n_execution_workers: 28,
  committer_cache_size: 1000000,
  proposal_timeout_max: 15.0,
  min_gas_price: 3000000000,
  audited_libfuncs_only: false,
  compare_retrospective_block_hash: false,
  central_sync_client_config: null,
  p2p_sync_client_config: {},
  state_sync_network_config: {
    port: 55010,
  },
}
