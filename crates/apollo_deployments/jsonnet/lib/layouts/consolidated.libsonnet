// Consolidated layout structure.
local catalog = import '../infra/catalog.libsonnet';
{
  reactive:: catalog.reactive,
  active:: catalog.active,
  // A single-process node serves nothing remotely, so every hosted reactive component is local-only.
  localOnly:: catalog.reactive,

  // Unused in this layout (no component is remote-served or consumed), kept for the descriptor
  // contract; `derive` only dereferences them for remote / remote-enabled components.
  ports:: {},
  serviceDns:: {},
  scale:: {},

  services:: {
    node: {
      runs: catalog.reactive + catalog.active,
      uses: [],
    },
  },
}
