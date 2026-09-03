// chain_params for the `testing/all-constructs` overlay (hybrid layout): clearly-dummy per-chain
// values plus dummy overrides of the applicative-config defaults, the minimum for native synth to
// succeed. NONE of these are deployed anywhere; the synth this overlay feeds is only `kubectl
// validate`d for manifest structure, never for config content.
{
  mandatory: {
    chain_id: 'SN_DUMMY',
    starknet_url: 'http://dummy-starknet/',
    recorder_url: 'http://dummy-recorder/',
    starknet_contract_address: '0x0',
    base_layer: {
      bpo1_start_block_number: 0,
      bpo2_start_block_number: 0,
      fusaka_no_bpo_start_block_number: 0,
    },
    staking_default_committee: {
      start_epoch: 0,
      committee_size: 10,
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
  // Dummy values; nothing here is deployed.
  native_classes_whitelist: 'All',
  n_concurrent_txs: 1,
  n_execution_workers: 1,
  committer_cache_size: 1000000,
  audited_libfuncs_only: false,
  central_sync_client_config: null,
  p2p_sync_client_config: {},
}
