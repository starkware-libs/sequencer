// Full per-service SequencerNodeConfig for the `testing/all-constructs` overlay (self-contained testing overlay).
// Evaluate: jsonnet -J <repo>/crates/apollo_deployments/jsonnet node.jsonnet
local build = import 'lib/build.libsonnet';
build.build({
  chain_params: import './chain_params.jsonnet',
  node_params: { validator_id: '0x64', node_index: 0 },
})
