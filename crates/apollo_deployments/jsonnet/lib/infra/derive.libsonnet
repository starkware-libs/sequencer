// Derives one service's `components` map from a layout descriptor. Layout-agnostic; the per-layout
// data lives in lib/layouts/<layout>.libsonnet. Node-wide values that are NOT topology
// (`monitoring_config`, `validation_only`) are assembled in lib/build.libsonnet, not here — this
// module emits only the infra `components`.
//
// A layout descriptor `L` provides:
//   reactive:   [name]                 — the reactive components (get a full execution config)
//   active:     [name]                 — the active components (execution_mode only)
//   ports:      {name: port}           — infra port of each remote-served reactive component
//   serviceDns: {service: dns}         — k8s service name hosting each service
//   homeOf:     {reactiveName: service}— which service hosts each remote-served reactive component
//   localOnly:  [reactiveName]         — hosted-but-not-remote-served (LocalExecutionWithRemoteDisabled)
//   scale:      {service: {retries, idle}} — remote-client tuning of components homed in that service
//   services:   {service: {runs: [name], uses: [reactiveName]}}
//                                      — components the service hosts (runs) / consumes remotely (uses)
//
// Mode of a reactive component `c` in service `svc`: c∈uses → Remote; else c∈runs → (c∈localOnly ?
// LocalExecutionWithRemoteDisabled : LocalExecutionWithRemoteEnabled); else Disabled. Active c∈runs →
// Enabled else Disabled. `url`/`port` of a remote-served component come from its home service, so they
// are identical wherever the component appears.
local cb = import 'component_block.libsonnet';

// url/port of a remote-served reactive component, resolved from its home service.
local hostUrl(L, component) = L.serviceDns[L.homeOf[component]];
local hostPort(L, component) = L.ports[component];

local reactiveEntry(L, svc, component) =
  local spec = L.services[svc];
  if std.member(spec.uses, component) then
    local home = L.homeOf[component];
    cb.remote(hostUrl(L, component), hostPort(L, component), L.scale[home].retries, L.scale[home].idle)
  else if std.member(spec.runs, component) then
    (if std.member(L.localOnly, component)
     then cb.localRemoteDisabled()
     else cb.localRemoteEnabled(hostUrl(L, component), hostPort(L, component)))
  else cb.disabledReactive();

local activeEntry(L, svc, component) =
  if std.member(L.services[svc].runs, component) then cb.activeEnabled() else cb.activeDisabled();

// The `components` map for one service: every reactive + active component, classified.
local componentsForImpl(L, svc) =
  local spec = L.services[svc];
  assert std.length(std.setInter(std.set(spec.runs), std.set(spec.uses))) == 0 :
    'layout service %s lists a component in both runs and uses' % svc;
  { [component]: reactiveEntry(L, svc, component) for component in L.reactive }
  + { [component]: activeEntry(L, svc, component) for component in L.active };

{
  // The `components` map for one service.
  componentsFor(L, svc):: componentsForImpl(L, svc),

  // Convenience wrapper used by the services/<layout>.jsonnet infra renderers (infra-only).
  infraFor(L, svc):: { components: componentsForImpl(L, svc) },
}
