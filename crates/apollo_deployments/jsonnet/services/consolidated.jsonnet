// Generates the `components` map for the consolidated layout.
local derive = import '../lib/infra/derive.libsonnet';
local consolidated = import '../lib/layouts/consolidated.libsonnet';

{
  [service]: derive.infraFor(consolidated, service)
  for service in std.objectFields(consolidated.services)
}
