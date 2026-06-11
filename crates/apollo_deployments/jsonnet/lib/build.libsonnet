// Top-level assembler: build the per-service SequencerNodeConfig for a whole layout.
//
//   build(layout, overrides) -> { [service]: <SequencerNodeConfig> }
//
// where `layout` is one of {consolidated, hybrid, distributed} and `overrides` is the object the
// applicative config reads its per-environment values from (see applicative_config.libsonnet).
// Every service of the layout carries the same business-logic config (the applicative config); only
// the infra `components` map differs per service. `validation_only` is sourced once from
// `overrides` — the applicative config sets the matching batcher pointee from the same source, so
// the CONFIG_POINTERS pair agrees by construction.
local derive = import 'infra/derive.libsonnet';
local applicative = import 'applicative_config.libsonnet';

local layouts = {
  consolidated: import 'layouts/consolidated.libsonnet',
  hybrid: import 'layouts/hybrid.libsonnet',
  distributed: import 'layouts/distributed.libsonnet',
};

// The applicative config groups each section under a component-name wrapper
// (`batcher: { batcher_config: {...} }`). SequencerNodeConfig deserializes from the inner
// `<x>_config` keys at the top level, so lift each group's body up one level. `signature_manager: {}`
// contributes nothing (no signature_manager_config field exists); `monitoring: { monitoring_config }`
// lifts `monitoring_config`.
local flattenApplicative(app) =
  std.foldl(function(acc, group) acc + app[group], std.objectFields(app), {});

{
  build(layout, overrides)::
    assert std.objectHas(layouts, layout) : 'unknown layout: %s' % layout;
    local L = layouts[layout];
    local businessConfig = flattenApplicative(applicative(overrides));
    {
      [service]: businessConfig
                 + { components: derive.componentsFor(L, service) }
                 + { validation_only: overrides.validation_only }
      for service in std.objectFields(L.services)
    },
}
