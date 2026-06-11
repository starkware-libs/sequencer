local defaultReplacers = import 'applicative_config/default_replacers.libsonnet';
local applicative = import 'applicative_config/init.libsonnet';
local derive = import 'infra/derive.libsonnet';

// `base_layer_config` is not a component, but is a field of SequencerNodeConfig.
// It must be provided when the service runs the l1 components (L1EventsScraper and
// L1GasPriceScraper).
local baseLayerComponents = ['l1_events_scraper', 'l1_gas_price_scraper'];

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
  build(params)::
    assert std.objectHas(params, 'chain_params') : 'params.chain_params is required';
    assert std.objectHas(params, 'node_params') : 'params.node_params is required';
    assert std.objectHas(params.chain_params, 'topology') : 'params.chain_params.topology is required';

    local topology = params.chain_params.topology;
    local chainParams = defaultReplacers + {
      [key]: params.chain_params[key]
      for key in std.objectFields(params.chain_params)
      if key != 'topology'
    };
    local nodeParams = params.node_params;
    local applicativeConfig = applicative(chainParams, nodeParams);
    {
      [service]: serviceConfig(applicativeConfig, topology.services[service].runs)
                 + { components: derive.componentsFor(topology, service) }
                 + { validation_only: chainParams.validation_only }
      for service in std.objectFields(topology.services)
    },
}
