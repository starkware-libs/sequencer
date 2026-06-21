// Test overlay layer — services/networking partition: the remaining 10 of the 20 top-level
// override keys. Together with `overlay_partial_base.libsonnet` it partitions the complete
// `overrides.libsonnet` set exactly (no gaps, no overlaps). Values are projected from the canonical
// fixture (imported relative to this file's own directory, exercising overlay-local import
// resolution) so the two partial overlays stay in sync with the complete set by construction.
local overrides = import 'overrides.libsonnet';
{
  consensus_manager_config: overrides.consensus_manager_config,
  gateway_config: overrides.gateway_config,
  http_server_config: overrides.http_server_config,
  mempool_config: overrides.mempool_config,
  mempool_p2p_config: overrides.mempool_p2p_config,
  monitoring_endpoint_config: overrides.monitoring_endpoint_config,
  sierra_compiler_config: overrides.sierra_compiler_config,
  state_sync_config: overrides.state_sync_config,
  recorder_url: overrides.recorder_url,
  starknet_url: overrides.starknet_url,
}
