"""Native (nested) node-config generation via jsonnet `build()`.

Each overlay's leaf dir holds a self-contained `node.jsonnet` that itself calls
`(import 'lib/build.libsonnet').build({ chain_params, node_params })`, where `chain_params.mandatory`
carries the topology (and the other no-fallback per-env values), and returns the
per-service config map. `build_native_config` evaluates a node's `node.jsonnet` (with the jsonnet
JPATH pointed at `crates/apollo_deployments/jsonnet`) and selects the requested service (mapping
e.g. `sierracompiler` -> `sierra_compiler`) as the ConfigMap. The caller resolves the node.jsonnet
path (it already holds the overlay chain).
"""

import json
from pathlib import Path
from typing import Any, Dict

import _jsonnet

REPO_ROOT = Path(__file__).resolve().parents[4]
JSONNET_DIR = REPO_ROOT / "crates" / "apollo_deployments" / "jsonnet"

# Overlay/deployment service names that differ from the jsonnet build key.
_SERVICE_NAME_TO_BUILD_KEY = {"sierracompiler": "sierra_compiler"}


def build_native_config(service_name: str, node_file: Path) -> Dict[str, Any]:
    """Return one service's nested `SequencerNodeConfig` by evaluating its `node.jsonnet`."""
    built = json.loads(_jsonnet.evaluate_file(str(node_file), jpathdir=[str(JSONNET_DIR)]))
    build_key = _SERVICE_NAME_TO_BUILD_KEY.get(service_name, service_name)
    if build_key not in built:
        raise ValueError(
            f"{node_file} produced no service '{build_key}' (for overlay service "
            f"'{service_name}'). Available services: {sorted(built.keys())}"
        )
    return built[build_key]
