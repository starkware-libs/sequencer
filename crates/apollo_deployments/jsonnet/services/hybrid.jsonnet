// Infra config for every service of the hybrid layout, keyed by service name (the `components` map).
// This is the infra-only renderer used by the infra-parity tests; the full per-service
// SequencerNodeConfig (applicative config + components + node-wide values) is assembled by
// lib/build.libsonnet.
local derive = import '../lib/infra/derive.libsonnet';
local hybrid = import '../lib/layouts/hybrid.libsonnet';

{
  [service]: derive.infraFor(hybrid, service)
  for service in std.objectFields(hybrid.services)
}
