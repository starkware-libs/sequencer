// Overrides layer for the `dummy_for_testing` overlay (hybrid layout).
// Supplies the dummy values the `prepare-production-overlays` CI job needs for env-only synth.
{
  validator_id: '0x64',

  consensus_manager_config: {
    network_config: {
      advertised_multiaddr: null,
      bootstrap_peer_multiaddr: null,
    },
  },

  mempool_p2p_config: {
    network_config: {
      advertised_multiaddr: null,
      bootstrap_peer_multiaddr: null,
    },
  },
}
