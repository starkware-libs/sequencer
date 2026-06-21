// Test overlay layer — base/infrastructure partition: the first 10 of the 20 top-level override
// keys. Together with `overlay_partial_services.libsonnet` it partitions the complete
// `overrides.libsonnet` set exactly (no gaps, no overlaps). Values are projected from the canonical
// fixture (imported relative to this file's own directory, exercising overlay-local import
// resolution) so the two partial overlays stay in sync with the complete set by construction.
local overrides = import 'overrides.libsonnet';
{
  chain_id: overrides.chain_id,
  eth_fee_token_address: overrides.eth_fee_token_address,
  strk_fee_token_address: overrides.strk_fee_token_address,
  validator_id: overrides.validator_id,
  native_classes_whitelist: overrides.native_classes_whitelist,
  versioned_constants_overrides: overrides.versioned_constants_overrides,
  base_layer_config: overrides.base_layer_config,
  batcher_config: overrides.batcher_config,
  class_manager_config: overrides.class_manager_config,
  committer_config: overrides.committer_config,
}
