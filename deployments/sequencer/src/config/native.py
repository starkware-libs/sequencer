"""Native (nested) node-config generation via jsonnet `build()`.

The legacy "preset" path fills `$$$_..._$$$` placeholders in flat dotted-key replacer JSON to
produce the ConfigMap. The "native" path instead assembles the nested `SequencerNodeConfig` the
node deserializes directly (loaded with `--config_format native`).

Pipeline:
  1. Locate the per-layer `*sequencer_config.jsonnet` override files along the SAME overlay chain the
     YAML loader resolves (base `common` layer < each `-o` overlay dir, in order). These layer files
     sit next to each overlay's `common.yaml`, including the cross-repo devops overlay dirs; a dir may
     hold more than one (e.g. the public applicative layer and the devops-owned P2P layer), all merged.
  2. Evaluate each layer file to JSON and deep-merge them (null-preserving, so an explicit `null`
     from a later layer stays null).
  3. Evaluate `(import 'lib/build.libsonnet').build('hybrid', <merged-overrides>)` with the jsonnet
     JPATH pointed at `crates/apollo_deployments/jsonnet`.
  4. Select `result[build_key(service_name)]` — the overlay service name mapped to the build key
     (notably `sierracompiler` -> `sierra_compiler`) — and use that nested object as the ConfigMap.
"""

import copy
import json
from pathlib import Path
from typing import Any, Dict, List, Optional

import _jsonnet

from src.config.overlays import overlay_dirs

# Repo root: this file is deployments/sequencer/src/config/native.py, root is 4 levels up.
REPO_ROOT = Path(__file__).resolve().parents[4]

# JPATH for the jsonnet evaluator, matching `crates/apollo_deployments/src/jsonnet.rs` (JSONNET_DIR).
# `import 'lib/build.libsonnet'` and build's internal relative imports resolve under this dir.
JSONNET_DIR = REPO_ROOT / "crates" / "apollo_deployments" / "jsonnet"

# Glob matching every override-layer file in an overlay layer directory. The pattern requires the
# `sequencer_config.jsonnet` suffix but allows a name prefix, so a directory may carry more than one
# layer file (e.g. the public repo's applicative `sequencer_config.jsonnet` and the devops-owned
# `common_sequencer_config.jsonnet`), authored with distinct names so the deploy's
# `cp -rf devops/overlays -> public/configs/overlays` (a file-level replace) cannot clobber one with
# the other. All matches in a dir are deep-merged in sorted order; layer files sharing a dir MUST
# have disjoint leaf keys (enforced by the cross-repo collision test) so the merge order is immaterial.
LAYER_GLOB = "*sequencer_config.jsonnet"

# Maps an overlay/deployment service name to the build key used by the jsonnet layout
# (`lib/layouts/hybrid.libsonnet` `services::`). Only `sierracompiler` differs from the build key;
# every other service name is identity.
_SERVICE_NAME_TO_BUILD_KEY = {"sierracompiler": "sierra_compiler"}


def service_name_to_build_key(service_name: str) -> str:
    """Map an overlay service name to the key `build('hybrid', ...)` emits.

    `sierracompiler` -> `sierra_compiler`; every other name maps to itself.
    """
    return _SERVICE_NAME_TO_BUILD_KEY.get(service_name, service_name)


def deep_merge_preserving_null(base: dict, overlay: dict) -> dict:
    """Recursively deep-merge `overlay` onto `base`, preserving explicit `null` values.

    `null` is a real value here (it folds an optional config to `None`), so a later layer setting a
    key to `null` must overwrite an earlier non-null value rather than being skipped. Two dicts merge
    key-by-key; any non-dict overlay value (including `None`/`null`) replaces the base value
    outright.

    Returns a new dict that is fully independent of `base` (deep-copied once, up front); neither
    input is mutated.
    """
    merged = copy.deepcopy(base)
    _merge_into(merged, overlay)
    return merged


def _merge_into(target: dict, overlay: dict) -> None:
    """Recursively merge `overlay` into `target`, MUTATING `target` in place.

    Assumes the caller owns `target` (e.g. a fresh deep copy from `deep_merge_preserving_null`), so
    it is free to write into it. Two dicts merge key-by-key; any non-dict overlay value (including
    explicit `null`) replaces the target value outright. `overlay` is only read, never mutated.
    """
    for key, overlay_value in overlay.items():
        target_value = target.get(key)
        if isinstance(target_value, dict) and isinstance(overlay_value, dict):
            _merge_into(target_value, overlay_value)
        else:
            # Non-dict (incl. explicit null) or new key: overlay wins outright.
            target[key] = overlay_value


def _dir_layer_files(layer_dir: Path) -> List[Path]:
    """Every `*sequencer_config.jsonnet` override-layer file in `layer_dir`, in sorted order.

    A single overlay dir may hold multiple layer files (distinct names so the deploy's file-level
    `cp -rf` cannot clobber one with the other across repos). They are merged in sorted order; layer
    files in the same dir must have disjoint leaf keys, so the order is immaterial (see `LAYER_GLOB`).
    """
    return sorted(layer_dir.glob(LAYER_GLOB))


def _expand_overlay_ancestors(overlays: List[str]) -> List[str]:
    """Expand each dotted overlay into its full ancestor prefix chain, root-to-leaf, deduped.

    A single overlay names only its LEAF dir, but the YAML loader resolves the whole chain of
    intermediate env dirs along the dotted path (e.g. the `sepolia-integration` env dir carries its
    own override layer between `common` and the per-node leaf). The native resolver must visit those
    same intermediate dirs, so expand each overlay `hybrid.a.b.c` into its prefixes
    `[hybrid.a, hybrid.a.b, hybrid.a.b.c]` (the bare layout name `hybrid` is dropped — that maps to
    the overlays root, not an overlay dir; the base `common` layer is handled separately).

    Prefixes are emitted in root-to-leaf order so the downstream deep-merge precedence stays
    base(common) < env < ... < leaf (last wins). When the caller passes multiple overlays that share
    ancestors (e.g. both `hybrid.a.b` and `hybrid.a.b.c`), the shared prefixes would repeat; dedup
    keeping the FIRST occurrence so each dir is globbed exactly once and the ordered output is
    identical whether the caller passes only the leaf or the full chain.
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


def resolve_layer_files(
    layout: str, overlays: List[str], base_dir: Optional[str] = None
) -> List[Path]:
    """Resolve the ordered list of existing `*sequencer_config.jsonnet` layer files.

    Mirrors the YAML overlay chain resolution in `app.py._get_config_paths`:
      - BASE layer: every `configs/overlays/<layout>/common/*sequencer_config.jsonnet`
      - Each `-o` overlay (dot notation, must start with the layout name) contributes every
        `<overlay dir>/*sequencer_config.jsonnet`, in the order the overlays were given. Each overlay
        is first expanded into its full ANCESTOR PREFIX CHAIN (`_expand_overlay_ancestors`), so the
        intermediate env dirs along the dotted path (which carry their own override layers) are
        visited too — not just the named leaf dir.

    Within one dir, multiple matches are returned in sorted order (they must have disjoint leaf keys;
    see `LAYER_GLOB`). `base_dir` is the directory under which `configs/overlays/...` resolves;
    defaults to the deployments/sequencer dir (matching `app._get_base_dir`). At deploy time the
    cross-repo devops overlay files are laid into this tree, so the env/per-node layers resolve here.

    Dirs with no matching layer file contribute nothing (an overlay layer need not carry applicative
    deltas). Returned in deep-merge order: base first, then each ancestor dir root-to-leaf (last
    wins). Shared ancestors across multiple overlays are deduped, so the output is identical whether
    the caller passes only the leaf overlay or the leaf plus its explicit ancestors.
    """
    base = Path(base_dir) if base_dir else Path(_config_base_dir())

    layer_files: List[Path] = []

    # BASE layer: configs/overlays/<layout>/common.
    layer_files.extend(_dir_layer_files(base / "configs" / "overlays" / layout / "common"))

    # Each overlay's full ancestor prefix chain (root-to-leaf, deduped), in the given order (last
    # wins). `overlay_dirs` does the dotted-path walk + layout-name validation shared with
    # `app._get_config_paths`; feeding it the expanded prefixes visits the intermediate env dirs too.
    for overlay_dir in overlay_dirs(base, layout, _expand_overlay_ancestors(overlays)):
        layer_files.extend(_dir_layer_files(overlay_dir))

    return layer_files


def merged_overrides(
    layout: str, overlays: List[str], base_dir: Optional[str] = None
) -> Dict[str, Any]:
    """Evaluate every layer file and deep-merge them (null-preserving) into a single overrides dict."""
    merged: Dict[str, Any] = {}
    for layer_file in resolve_layer_files(layout, overlays, base_dir=base_dir):
        layer = _eval_jsonnet_file(layer_file)
        if not isinstance(layer, dict):
            raise ValueError(
                f"Override layer '{layer_file}' must evaluate to a JSON object, got "
                f"{type(layer).__name__}"
            )
        merged = deep_merge_preserving_null(merged, layer)
    return merged


def build_native_config(
    service_name: str,
    layout: str,
    overlays: List[str],
    extra_overrides: Optional[Dict[str, Any]] = None,
    base_dir: Optional[str] = None,
) -> Dict[str, Any]:
    """Assemble the nested `SequencerNodeConfig` for one service via jsonnet `build()`.

    `extra_overrides` are deep-merged (null-preserving) over the file-resolved overrides as the
    last layer; callers pass the service's own YAML `sequencerConfig` deltas here if any.
    `base_dir` overrides the overlay resolution root (see `resolve_layer_files`).
    """
    overrides = merged_overrides(layout, overlays, base_dir=base_dir)
    if extra_overrides:
        overrides = deep_merge_preserving_null(overrides, extra_overrides)

    built = _eval_build(layout, overrides)

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


def _eval_build(layout: str, overrides: Dict[str, Any]) -> Dict[str, Any]:
    """Evaluate `(import 'lib/build.libsonnet').build(<layout>, <overrides>)` and return its JSON.

    Matches the Rust eval form (`crates/apollo_deployments/src/jsonnet.rs`): `build` is a method on
    the imported object (no extVar / TLA). The overrides object is injected inline as a JSON literal.
    """
    snippet = "(import 'lib/build.libsonnet').build(%s, %s)" % (
        json.dumps(layout),
        json.dumps(overrides),
    )
    rendered = _jsonnet.evaluate_snippet("build_native", snippet, jpathdir=[str(JSONNET_DIR)])
    return json.loads(rendered)
