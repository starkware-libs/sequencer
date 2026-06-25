"""Native (nested) node-config generation via jsonnet `build()`.

The legacy "preset" path fills `$$$_..._$$$` placeholders in flat dotted-key replacer JSON to
produce the ConfigMap. The "native" path instead assembles the nested `SequencerNodeConfig` the
node deserializes directly from its `--config_file`(s).

Pipeline:
  1. For each override bucket (`chain_params`, `node_params`, `replacers`), locate its file along the
     SAME overlay chain the YAML loader resolves (base `common` layer < each `-o` overlay dir, in
     order), including the cross-repo devops overlay dirs. Within a dir the private-repo (devops)
     `private_<bucket>.jsonnet` wins over the public-repo (sequencer) `<bucket>.jsonnet`: the private
     file `(import './<bucket>.jsonnet') + { … }` composes the public base NATIVELY in jsonnet, so
     the cross-repo merge is a jsonnet import, not a Python merge (see `_dir_bucket_file`).
  2. The deepest overlay level that defines a bucket wins (it composes shallower layers via jsonnet
     import); evaluate that one file per bucket, assembling `params = { chain_params, node_params,
     replacers }`.
  3. Evaluate `(import 'lib/build.libsonnet').build('hybrid', <params>)` with the jsonnet JPATH
     pointed at `crates/apollo_deployments/jsonnet`.
  4. Select `result[build_key(service_name)]` — the overlay service name mapped to the build key
     (notably `sierracompiler` -> `sierra_compiler`) — and use that nested object as the ConfigMap.
"""

import json
from pathlib import Path
from typing import Any, Dict, List, Optional

import _jsonnet
from src.config.overlays import overlay_dirs

REPO_ROOT = Path(__file__).resolve().parents[4]
JSONNET_DIR = REPO_ROOT / "crates" / "apollo_deployments" / "jsonnet"

# The three override buckets `build()` consumes. Each overlay layer supplies a bucket as either the
# public `<bucket>.jsonnet` or the private `private_<bucket>.jsonnet`; the deepest layer that defines
# it wins (composing shallower layers / the public base via jsonnet import).
BUCKETS = ("chain_params", "node_params", "replacers")

# Maps an overlay/deployment service name to the build key used by the jsonnet layout.
_SERVICE_NAME_TO_BUILD_KEY = {"sierracompiler": "sierra_compiler"}


def service_name_to_build_key(service_name: str) -> str:
    """Map an overlay service name to the key `build('hybrid', ...)` emits.

    `sierracompiler` -> `sierra_compiler`; every other name maps to itself.
    """
    return _SERVICE_NAME_TO_BUILD_KEY.get(service_name, service_name)


def _dir_bucket_file(layer_dir: Path, filename: str) -> Optional[Path]:
    """Returns the bucket file `layer_dir` contributes for `filename` (e.g. `chain_params.jsonnet`).

    The private-repo (devops) `private_<filename>` wins over the public-repo (sequencer) `<filename>`
    when both are present in the dir (both survive the deploy-time overlay copy — different names).
    The private file is expected to `(import './<filename>') + { … }`, composing the public base in
    jsonnet, so native reads only the private file and the cross-repo merge stays native. Returns
    `None` if the dir contributes neither.
    """
    private = layer_dir / f"private_{filename}"
    if private.is_file():
        return private
    base = layer_dir / filename
    return base if base.is_file() else None


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
    """Resolves the ordered list of `<filename>` bucket files along the overlay chain, shallowest
    (base `common`) to deepest (leaf overlay dir). Each dir contributes at most one file, private
    preferred over public (see `_dir_bucket_file`); `build_params` takes the deepest.
    """
    base = Path(base_dir) if base_dir else Path(_config_base_dir())

    dirs = [base / "configs" / "overlays" / layout / "common"]
    # Each overlay's full ancestor prefix chain (root-to-leaf, deduped), in the given order.
    # `overlay_dirs` does the dotted-path walk + layout-name validation shared with
    # `app._get_config_paths`; feeding it the expanded prefixes visits the intermediate env dirs too.
    dirs.extend(overlay_dirs(base, layout, _expand_overlay_ancestors(overlays)))

    return [file for file in (_dir_bucket_file(layer_dir, filename) for layer_dir in dirs) if file]


def build_params(
    layout: str, overlays: List[str], base_dir: Optional[str] = None
) -> Dict[str, Any]:
    """Assemble the bucketed `params` object for `build()` from the per-bucket overlay files.

    For each bucket in `BUCKETS`, the deepest overlay level that defines it wins (that file composes
    any shallower layer / the public base via jsonnet import). A bucket with no file is omitted, so
    `build()` applies its inline defaults (only `replacers` has defaults; `chain_params`/`node_params`
    are mandatory and must be supplied by some layer in the chain).
    """
    params: Dict[str, Any] = {}
    for bucket in BUCKETS:
        files = resolve_bucket_files(layout, overlays, f"{bucket}.jsonnet", base_dir=base_dir)
        if not files:
            continue
        bucket_file = files[-1]  # deepest overlay level wins
        layer = _eval_jsonnet_file(bucket_file)
        if not isinstance(layer, dict):
            raise ValueError(
                f"Bucket file '{bucket_file}' must evaluate to a JSON object, got "
                f"{type(layer).__name__}"
            )
        params[bucket] = layer
    return params


def build_native_config(
    service_name: str,
    layout: str,
    overlays: List[str],
    base_dir: Optional[str] = None,
) -> Dict[str, Any]:
    """Assemble the nested `SequencerNodeConfig` for one service via jsonnet `build()`.

    `base_dir` overrides the overlay resolution root (see `resolve_bucket_files`).
    """
    params = build_params(layout, overlays, base_dir=base_dir)
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
