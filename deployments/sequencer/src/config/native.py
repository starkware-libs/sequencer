"""Native (nested) node-config generation via jsonnet `build()`.

Each overlay's leaf dir holds a self-contained `node.jsonnet` that itself calls
`(import 'lib/build.libsonnet').build(topology, { chain_params, node_params })` — importing its own
chain_params + topology and inlining its node_params — and returns the per-service config map.

`build_native_config` locates the leaf `-o` overlay's `node.jsonnet`, evaluates it with the jsonnet
JPATH pointed at `crates/apollo_deployments/jsonnet`, and selects `result[build_key(service_name)]`
(mapping e.g. `sierracompiler` -> `sierra_compiler`) as the ConfigMap.
"""

import json
from pathlib import Path
from typing import Any, Dict, List, Optional

import _jsonnet
from src.config.overlays import overlay_dirs

REPO_ROOT = Path(__file__).resolve().parents[4]
JSONNET_DIR = REPO_ROOT / "crates" / "apollo_deployments" / "jsonnet"

# Maps an overlay/deployment service name to the build key used by the jsonnet layout.
_SERVICE_NAME_TO_BUILD_KEY = {"sierracompiler": "sierra_compiler"}


def service_name_to_build_key(service_name: str) -> str:
    """Map an overlay service name to the key `build(<topology>, ...)` emits.

    `sierracompiler` -> `sierra_compiler`; every other name maps to itself.
    """
    return _SERVICE_NAME_TO_BUILD_KEY.get(service_name, service_name)


def _expand_overlay_ancestors(overlays: List[str]) -> List[str]:
    """
    Expand each dotted overlay into its full ancestor prefix chain, root-to-leaf, deduped.
    """
    expanded: List[str] = []
    seen = set()
    for overlay in overlays:
        segments = overlay.split(".")
        # Build each prefix from the layout root down to the leaf; skip the bare-layout prefix
        # (segments[:1]), which is the overlays root rather than an overlay dir.
        for end in range(2, len(segments) + 1):
            prefix = ".".join(segments[:end])
            if prefix not in seen:
                seen.add(prefix)
                expanded.append(prefix)
    return expanded


def build_native_config(
    service_name: str,
    layout: str,
    overlays: List[str],
    base_dir: Optional[str] = None,
) -> Dict[str, Any]:
    """Return one service's nested `SequencerNodeConfig` by evaluating the node's `node.jsonnet`.

    `base_dir` overrides the overlay resolution root (see `_node_jsonnet_path`).
    """
    node_file = _node_jsonnet_path(layout, overlays, base_dir=base_dir)
    built = _eval_jsonnet_file(node_file)

    build_key = service_name_to_build_key(service_name)
    if build_key not in built:
        raise ValueError(
            f"{node_file} produced no service '{build_key}' (for overlay service "
            f"'{service_name}'). Available services: {sorted(built.keys())}"
        )
    return built[build_key]


def _node_jsonnet_path(
    layout: str, overlays: List[str], base_dir: Optional[str] = None
) -> Path:
    """Path to the leaf overlay's `node.jsonnet` (the deepest `-o` overlay dir)."""
    if not overlays:
        raise ValueError("build_native_config requires at least one overlay to locate node.jsonnet")
    base = Path(base_dir) if base_dir else Path(_config_base_dir())
    node_dir = overlay_dirs(base, layout, _expand_overlay_ancestors(overlays))[-1]
    node_file = node_dir / "node.jsonnet"
    if not node_file.is_file():
        raise ValueError(f"no node.jsonnet in the leaf overlay dir {node_dir}")
    return node_file


def _config_base_dir() -> str:
    """Base dir under which overlays resolve, matching `app.py._get_base_dir` (deployments/sequencer)."""
    # native.py is deployments/sequencer/src/config/native.py; deployments/sequencer is 2 levels up.
    return str(Path(__file__).resolve().parents[2])


def _eval_jsonnet_file(path: Path) -> Any:
    """Evaluate a jsonnet file to a Python object, with imports resolved relative to its own dir."""
    rendered = _jsonnet.evaluate_file(str(path), jpathdir=[str(JSONNET_DIR)])
    return json.loads(rendered)
