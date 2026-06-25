// Full per-service SequencerNodeConfig for the `testing/node-0` overlay (self-contained testing overlay).
// Evaluate: jsonnet -J <repo>/crates/apollo_deployments/jsonnet node.jsonnet
local build = import 'lib/build.libsonnet';
build.build({
  chain_params: (import './chain_params.jsonnet') + { topology: import './topology.jsonnet' },
  node_params: { validator_id: '0x64', consensus_advertised_multiaddr: null, mempool_advertised_multiaddr: null },
})
