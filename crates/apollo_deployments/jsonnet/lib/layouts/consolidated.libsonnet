// Consolidated layout (a single `node` service running every component locally): pure data consumed
// by lib/infra/derive.libsonnet. Mirrors crates/apollo_deployments/src/deployments/consolidated.rs
// (`get_consolidated_config`): every reactive component is LocalExecutionWithRemoteDisabled and every
// active component is Enabled. Nothing is remote-served or consumed, so there are no ports/DNS.
local catalog = import '../infra/catalog.libsonnet';
{
  reactive:: catalog.reactive,
  active:: catalog.active,
  // A single-process node serves nothing remotely, so every hosted reactive component is local-only.
  localOnly:: catalog.reactive,

  // Unused in this layout (no component is remote-served or consumed), kept for the descriptor
  // contract; `derive` only dereferences them for remote / remote-enabled components.
  ports:: {},
  serviceDns:: {},
  homeOf:: {},
  scale:: {},

  services:: {
    node: {
      runs: catalog.reactive + catalog.active,
      uses: [],
    },
  },
}
