// Top-level assembler: build the per-service SequencerNodeConfig for a whole layout.
//
//   build(layoutName, overrides) -> { [service]: <SequencerNodeConfig> }
//
// where `layoutName` is one of {consolidated, hybrid, distributed} and `overrides` is the object
// the applicative config reads its per-environment values from (see applicative_config.libsonnet).
local applicative = import 'applicative_config.libsonnet';
local constants = import 'constants.libsonnet';
local derive = import 'infra/derive.libsonnet';

local layouts = {
  consolidated: import 'layouts/consolidated.libsonnet',
  hybrid: import 'layouts/hybrid.libsonnet',
  distributed: import 'layouts/distributed.libsonnet',
};

// `base_layer_config` is not a component, but is a field of SequencerNodeConfig.
// It must be provided when the service runs the l1 components (L1EventsScraper and
// L1GasPriceScraper).
local baseLayerComponents = ['l1_events_scraper', 'l1_gas_price_scraper'];

// One service's applicative sections. The node requires each `<component>_config` to be set iff
// that component runs within that service. The l1 components also require `base_layer_config` to be
// present. All services use the same `monitoring_config`.
local serviceConfig(applicativeConfig, runs) =
  {
    [component + '_config']: applicativeConfig[component + '_config']
    for component in runs
    if std.objectHas(applicativeConfig, component + '_config')
  }
  + (if std.length(std.setInter(std.set(runs), std.set(baseLayerComponents))) > 0
     then { base_layer_config: applicativeConfig.base_layer_config }
     else {})
  + { monitoring_config: applicativeConfig.monitoring_config };

{
  build(layoutName, overrides)::
    assert std.objectHas(layouts, layoutName) : 'unknown layout: %s' % layoutName;
    local layout = layouts[layoutName];
    local applicativeConfig = applicative(overrides);
    {
      [service]: serviceConfig(applicativeConfig, layout.services[service].runs)
                 + { components: derive.componentsFor(layout, service) }
                 + { validation_only: std.get(overrides, 'validation_only', constants.DEFAULT_VALIDATION_ONLY) }
      for service in std.objectFields(layout.services)
    },
}
