"""Native (nested) node-config generation via jsonnet `build()`.

The legacy "preset" path fills `$$$_..._$$$` placeholders in flat dotted-key replacer JSON to
produce the ConfigMap. The "native" path instead assembles the nested `SequencerNodeConfig` the
node deserializes directly from its `--config_file`(s).

Pipeline:
  1. For each override bucket (`chain_params`, `node_params`, `replacers`), locate its per-layer
     `<bucket>.jsonnet` files along the SAME overlay chain the YAML loader resolves (base `common`
     layer < each `-o` overlay dir, in order), including the cross-repo devops overlay dirs.
  2. Evaluate and deep-merge each bucket's files (null-preserving, so an explicit `null` from a later
     layer stays null), assembling `params = { chain_params, node_params, replacers }`.
  3. Evaluate `(import 'lib/build.libsonnet').build('hybrid', <params>)` with the jsonnet JPATH
     pointed at `crates/apollo_deployments/jsonnet`.
  4. Select `result[build_key(service_name)]` — the overlay service name mapped to the build key
     (notably `sierracompiler` -> `sierra_compiler`) — and use that nested object as the ConfigMap.
"""

import copy
import json
from pathlib import Path
from typing import Any, Dict, List, Optional

import _jsonnet
from src.config.overlays import overlay_dirs

REPO_ROOT = Path(__file__).resolve().parents[4]
JSONNET_DIR = REPO_ROOT / "crates" / "apollo_deployments" / "jsonnet"

# The three override buckets `build()` consumes. Each overlay layer supplies them as separate
# `<bucket>.jsonnet` files, deep-merged (null-preserving) along the overlay chain.
BUCKETS = ("chain_params", "node_params", "replacers")

# Maps an overlay/deployment service name to the build key used by the jsonnet layout.
_SERVICE_NAME_TO_BUILD_KEY = {"sierracompiler": "sierra_compiler"}


def service_name_to_build_key(service_name: str) -> str:
    """Map an overlay service name to the key `build('hybrid', ...)` emits.

    `sierracompiler` -> `sierra_compiler`; every other name maps to itself.
    """
    return _SERVICE_NAME_TO_BUILD_KEY.get(service_name, service_name)


def flatten_dotted(nested: dict, prefix: str = "") -> Dict[str, Any]:
    """Flatten a nested config to dotted keys. Lists and null are leaf values (not recursed)."""
    flat: Dict[str, Any] = {}
    for key, value in nested.items():
        dotted = f"{prefix}{key}"
        if isinstance(value, dict):
            flat.update(flatten_dotted(value, prefix=f"{dotted}."))
        else:
            flat[dotted] = value
    return flat


def deep_merge_preserving_null(base: dict, overlay: dict) -> dict:
    """
    Recursively deep-merge `overlay` onto `base`, preserving explicit `null` values.
    """
    merged = copy.deepcopy(base)
    _merge_into(merged, overlay)
    return merged


def _merge_into(target: dict, overlay: dict) -> None:
    """
    Recursively merge `overlay` into `target`, MUTATING `target` in place.
    """
    for key, overlay_value in overlay.items():
        target_value = target.get(key)
        if isinstance(target_value, dict) and isinstance(overlay_value, dict):
            _merge_into(target_value, overlay_value)
        else:
            # Non-dict (incl. explicit null) or new key: overlay wins outright.
            target[key] = overlay_value


def _dir_bucket_file(layer_dir: Path, filename: str) -> List[Path]:
    """Returns `[<layer_dir>/<filename>]` if that bucket file exists, else `[]`."""
    path = layer_dir / filename
    return [path] if path.is_file() else []


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


def resolve_bucket_files(
    layout: str, overlays: List[str], filename: str, base_dir: Optional[str] = None
) -> List[Path]:
    """Resolves the ordered list of existing `<filename>` bucket files along the overlay chain."""
    base = Path(base_dir) if base_dir else Path(_config_base_dir())

    files: List[Path] = []

    # BASE layer: configs/overlays/<layout>/common.
    files.extend(_dir_bucket_file(base / "configs" / "overlays" / layout / "common", filename))

    # Each overlay's full ancestor prefix chain (root-to-leaf, deduped), in the given order (last
    # wins). `overlay_dirs` does the dotted-path walk + layout-name validation shared with
    # `app._get_config_paths`; feeding it the expanded prefixes visits the intermediate env dirs too.
    for overlay_dir in overlay_dirs(base, layout, _expand_overlay_ancestors(overlays)):
        files.extend(_dir_bucket_file(overlay_dir, filename))

    return files


def build_params(
    layout: str, overlays: List[str], base_dir: Optional[str] = None
) -> Dict[str, Any]:
    """Assemble the bucketed `params` object for `build()` from the per-bucket overlay files.

    For each bucket in `BUCKETS`, deep-merges its `<bucket>.jsonnet` files along the overlay chain
    (base `common` < each overlay, last wins). A bucket with no files is omitted, so `build()`
    applies its inline defaults (only `replacers` has defaults; `chain_params`/`node_params` are
    mandatory and must be supplied by some layer in the chain).
    """
    params: Dict[str, Any] = {}
    for bucket in BUCKETS:
        merged: Dict[str, Any] = {}
        found = False
        for bucket_file in resolve_bucket_files(
            layout, overlays, f"{bucket}.jsonnet", base_dir=base_dir
        ):
            layer = _eval_jsonnet_file(bucket_file)
            if not isinstance(layer, dict):
                raise ValueError(
                    f"Bucket file '{bucket_file}' must evaluate to a JSON object, got "
                    f"{type(layer).__name__}"
                )
            merged = deep_merge_preserving_null(merged, layer)
            found = True
        if found:
            params[bucket] = merged
    return params


def build_native_config(
    service_name: str,
    layout: str,
    overlays: List[str],
    extra_params: Optional[Dict[str, Any]] = None,
    base_dir: Optional[str] = None,
) -> Dict[str, Any]:
    """Assemble the nested `SequencerNodeConfig` for one service via jsonnet `build()`.

    `extra_params` are deep-merged (null-preserving) over the file-resolved bucketed params
    ({chain_params, node_params, replacers}). `base_dir` overrides the overlay resolution root
    (see `resolve_bucket_files`).
    """
    params = build_params(layout, overlays, base_dir=base_dir)
    if extra_params:
        params = deep_merge_preserving_null(params, extra_params)

    built = _eval_build(layout, params)

    build_key = service_name_to_build_key(service_name)
    if build_key not in built:
        raise ValueError(
            f"build('{layout}', ...) produced no service '{build_key}' (for overlay service "
            f"'{service_name}'). Available services: {sorted(built.keys())}"
        )
    return built[build_key]


def _config_base_dir() -> str:
    """Base dir under which overlays resolve, matching `app.py._get_base_dir` (deployments/sequencer)."""
    # native.py is deployments/sequencer/src/config/native.py; deployments/sequencer is 2 levels up.
    return str(Path(__file__).resolve().parents[2])


def _eval_jsonnet_file(path: Path) -> Any:
    """Evaluate a jsonnet file to a Python object, with imports resolved relative to its own dir."""
    rendered = _jsonnet.evaluate_file(str(path), jpathdir=[str(JSONNET_DIR)])
    return json.loads(rendered)


def _eval_build(layout: str, params: Dict[str, Any]) -> Dict[str, Any]:
    """Evaluates `(import 'lib/build.libsonnet').build(<layout>, <params>)` and returns its JSON."""
    snippet = "(import 'lib/build.libsonnet').build(%s, %s)" % (
        json.dumps(layout),
        json.dumps(params),
    )
    rendered = _jsonnet.evaluate_snippet("build_native", snippet, jpathdir=[str(JSONNET_DIR)])
    return json.loads(rendered)
