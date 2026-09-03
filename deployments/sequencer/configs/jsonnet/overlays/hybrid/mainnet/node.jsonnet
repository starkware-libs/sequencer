// CI stand-in: full per-service config for this env synthed WITHOUT the devops per-node layer, used by
// the prepare-production-overlays CI job. Composes the env's real chain_params (topology included) with
// the shared dummy node-level stand-ins. The real per-node config comes from the devops repo's node.jsonnet at deploy.
local build = import 'lib/build.libsonnet';
local dummy_multiaddrs = import '../common/dummy_for_testing/chain_params.jsonnet';
local dummy_node_params = import '../common/dummy_for_testing/node_params.jsonnet';
build.build({
  chain_params: (import './chain_params.jsonnet') + dummy_multiaddrs,
  node_params: dummy_node_params,
})
