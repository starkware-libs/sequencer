// Generates the `components` map for every service of the distributed layout.
local derive = import '../lib/infra/derive.libsonnet';
local distributed = import '../lib/layouts/distributed.libsonnet';

{
  [service]: derive.infraFor(distributed, service)
  for service in std.objectFields(distributed.services)
}
