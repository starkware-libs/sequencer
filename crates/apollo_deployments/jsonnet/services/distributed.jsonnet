// Infra config for every service of the distributed layout, keyed by service name (the `components`
// map). This is the infra-only renderer used by the infra-parity tests; the full per-service
// SequencerNodeConfig is assembled by lib/build.libsonnet.
local derive = import '../lib/infra/derive.libsonnet';
local distributed = import '../lib/layouts/distributed.libsonnet';

{
  [service]: derive.infraFor(distributed, service)
  for service in std.objectFields(distributed.services)
}
