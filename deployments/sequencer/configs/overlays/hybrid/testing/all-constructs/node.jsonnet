// Full per-service SequencerNodeConfig for the `testing/all-constructs` overlay (self-contained testing overlay).
// Evaluate: jsonnet -J <repo>/crates/apollo_deployments/jsonnet node.jsonnet
local build = import 'lib/build.libsonnet';
build.build(import './topology.jsonnet', {
  chain_params: import './chain_params.jsonnet',
  node_params: { validator_id: '0x64' },
})
