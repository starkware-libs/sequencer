// chain_params for the `dummy_for_testing` overlay (hybrid layout).
// Supplies the dummy env-shared P2P multiaddrs the `prepare-production-overlays` CI job needs for
// env-only synth (standing in for the devops env-common layer). The remaining chain_params come from
// the env overlay this layer is merged onto.
{
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
