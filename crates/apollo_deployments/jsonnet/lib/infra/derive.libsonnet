// Derives one service's `components` map from a layout descriptor. Layout-agnostic; the per-layout
// data lives in lib/layouts/<layout>.libsonnet.
local component_block = import 'component_block.libsonnet';

// The service hosting each remote-served reactive component, derived by inverting services[*].runs.
local homeOf(layout) = {
  [component]: service
  for service in std.objectFields(layout.services)
  for component in layout.services[service].runs
  if std.member(layout.reactive, component) && !std.member(layout.localOnly, component)
};

local reactiveEntry(layout, homeByComponent, service, component) =
  local serviceComponents = layout.services[service];
  local home = homeByComponent[component];
  local url = layout.serviceDns[home];
  local port = layout.ports[component];
  if std.member(serviceComponents.uses, component) then
    component_block.remote(url, port, layout.scale[home].retries, layout.scale[home].idle)
  else if std.member(serviceComponents.runs, component) then
    (if std.member(layout.localOnly, component)
     then component_block.localRemoteDisabled()
     else component_block.localRemoteEnabled(url, port))
  else component_block.disabledReactive();

local activeEntry(layout, service, component) =
  if std.member(layout.services[service].runs, component)
  then component_block.activeEnabled()
  else component_block.activeDisabled();

// The `components` map for one service: every reactive + active component, classified.
local componentsForImpl(layout, service) =
  local homeByComponent = homeOf(layout);
  local serviceComponents = layout.services[service];
  local runsAndUsesOverlap =
    std.setInter(std.set(serviceComponents.runs), std.set(serviceComponents.uses));
  assert std.length(runsAndUsesOverlap) == 0 :
         'layout service %s lists a component in both runs and uses' % service;
  {
    [component]: reactiveEntry(layout, homeByComponent, service, component)
    for component in layout.reactive
  }
  + {
    [component]: activeEntry(layout, service, component)
    for component in layout.active
  };

{
  // The `components` map for one service.
  componentsFor(layout, service):: componentsForImpl(layout, service),

  // Convenience wrapper used by the services/<layout>.jsonnet infra renderers (infra-only).
  infraFor(layout, service):: { components: componentsForImpl(layout, service) },
}
