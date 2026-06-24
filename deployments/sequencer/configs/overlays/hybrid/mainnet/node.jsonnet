// CI stand-in: full per-service config for this env synthed WITHOUT the devops per-node layer.
// Composes the env's real chain_params with DUMMY node-level values (null multiaddrs + a dummy
// validator_id). The real per-node config comes from the devops repo's node.jsonnet at deploy.
// Evaluate: jsonnet -J <repo>/crates/apollo_deployments/jsonnet node.jsonnet
local build = import 'lib/build.libsonnet';
local dummy_multiaddrs = {
  consensus_advertised_multiaddr: null,
  consensus_bootstrap_peer_multiaddr: null,
  mempool_advertised_multiaddr: null,
  mempool_bootstrap_peer_multiaddr: null,
};
build.build(import './topology.jsonnet', {
  chain_params: (import './chain_params.jsonnet') + dummy_multiaddrs,
  node_params: { validator_id: '0x64' },
})
