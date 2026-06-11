// Generates the `components` map for every service of the hybrid layout.
local derive = import '../lib/infra/derive.libsonnet';
local hybrid = import '../lib/layouts/hybrid.libsonnet';

{
  [service]: derive.infraFor(hybrid, service)
  for service in std.objectFields(hybrid.services)
}
