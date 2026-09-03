// Dummy env-shared P2P multiaddr stand-ins for the `prepare-production-overlays` CI job's env-only
// native synth (standing in for the devops env-common layer). Flat keys, matching what `build` reads.
// Imported by each env's node.jsonnet and merged onto the env's real chain_params.
{
  consensus_bootstrap_peer_multiaddr: null,
  mempool_bootstrap_peer_multiaddr: null,
}
