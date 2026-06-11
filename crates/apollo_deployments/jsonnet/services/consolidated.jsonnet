// Infra config for the consolidated layout, keyed by service name (a single `node`; the `components`
// map). This is the infra-only renderer used by the infra-parity tests; the full per-service
// SequencerNodeConfig is assembled by lib/build.libsonnet.
local derive = import '../lib/infra/derive.libsonnet';
local consolidated = import '../lib/layouts/consolidated.libsonnet';

{
  [service]: derive.infraFor(consolidated, service)
  for service in std.objectFields(consolidated.services)
}
