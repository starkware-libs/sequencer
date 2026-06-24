"""Tests for the native (nested) node-config generation path.

Covers:
  - the null-preserving deep-merge,
  - the per-layer lockstep test: each layer's `sequencer_config.jsonnet` mirrors the combined
    `config.sequencerConfig` of that layer's YAMLs (folded).
"""

import json
from pathlib import Path

import _jsonnet
import yaml
from src.config.native import JSONNET_DIR, deep_merge_preserving_null

DEPLOYMENTS_SEQUENCER = Path(__file__).resolve().parents[1]

LAYOUT = "hybrid"

# Per-layer override-file dirs: each layer holds a `sequencer_config.jsonnet` that must mirror the
# combined `config.sequencerConfig` of the YAMLs in the same dir (see
# `_assert_layer_jsonnet_mirrors_combined_yaml`).
HYBRID_OVERLAYS_DIR = DEPLOYMENTS_SEQUENCER / "configs" / "overlays" / LAYOUT
COMMON_LAYER_DIR = HYBRID_OVERLAYS_DIR / "common"
INTEGRATION_LAYER_DIR = HYBRID_OVERLAYS_DIR / "sepolia-integration"
SEPOLIA_ALPHA_LAYER_DIR = HYBRID_OVERLAYS_DIR / "sepolia-alpha"


def test_deep_merge_preserves_explicit_null():
    base = {"a": {"x": 1, "y": 2}, "b": "keep"}
    overlay = {"a": {"y": None}}
    merged = deep_merge_preserving_null(base, overlay)
    assert merged == {"a": {"x": 1, "y": None}, "b": "keep"}


def test_deep_merge_nested_objects():
    base = {"outer": {"inner": {"a": 1}}, "top": 0}
    overlay = {"outer": {"inner": {"b": 2}, "sibling": 3}}
    merged = deep_merge_preserving_null(base, overlay)
    assert merged == {"outer": {"inner": {"a": 1, "b": 2}, "sibling": 3}, "top": 0}


def test_deep_merge_non_dict_overwrites_dict():
    """A scalar in the overlay replaces a dict in the base outright."""
    base = {"k": {"nested": 1}}
    overlay = {"k": "scalar"}
    assert deep_merge_preserving_null(base, overlay) == {"k": "scalar"}


def test_deep_merge_does_not_mutate_inputs():
    base = {"a": {"x": 1}}
    overlay = {"a": {"y": 2}}
    deep_merge_preserving_null(base, overlay)
    assert base == {"a": {"x": 1}}  # Verify base is not mutated.
    assert overlay == {"a": {"y": 2}}  # Verify overlay is not mutated.


def _flatten(nested: dict, prefix: str = "") -> dict:
    """Flatten a nested config to dotted keys. Lists and null are leaf values (not recursed)."""
    flat = {}
    for key, value in nested.items():
        dotted = f"{prefix}{key}"
        if isinstance(value, dict):
            flat.update(_flatten(value, prefix=f"{dotted}."))
        else:
            flat[dotted] = value
    return flat


def _is_under(prefix: str, dotted_key: str) -> bool:
    """True if `dotted_key` equals `prefix` or is nested under it (segment-aligned)."""
    return dotted_key == prefix or dotted_key.startswith(prefix + ".")


def _combined_layer_sequencer_config(layer_dir: Path) -> dict:
    """Merge the flat-dotted `config.sequencerConfig` across one overlay layer's YAMLs.

    Mirrors what the preset path merges for a single layer: `<layer>/common.yaml` first, then each
    `<layer>/services/*.yaml` in sorted order (last wins). Returns the merged flat dotted-key dict
    (still carrying `.#is_none` markers and `components.*`; fold/drop them with
    `_fold_is_none_drop_components`).
    """
    merged: dict = {}
    files = []
    common_yaml = layer_dir / "common.yaml"
    if common_yaml.exists():
        files.append(common_yaml)
    services_dir = layer_dir / "services"
    if services_dir.is_dir():
        files.extend(sorted(services_dir.glob("*.yaml")))
    for yaml_file in files:
        document = yaml.safe_load(yaml_file.read_text()) or {}
        merged.update((document.get("config") or {}).get("sequencerConfig") or {})
    return merged


def _fold_is_none_drop_components(flat: dict) -> dict:
    """Apply the same transform the jsonnet override layers encode, to a flat YAML sequencerConfig.

    - drop every `components.*` key (the layout supplies components);
    - `<path>.#is_none: true`  -> `<path>: null` and drop the whole `<path>.*` subtree;
    - `<path>.#is_none: false` -> drop only the marker, keep the real sub-keys.
    Returns a flat dotted dict directly comparable to the flattened jsonnet layer.
    """
    none_true_roots = [
        key[: -len(".#is_none")]
        for key, value in flat.items()
        if key.endswith(".#is_none") and value is True
    ]
    folded = {}
    for key, value in flat.items():
        if key.split(".", 1)[0] == "components":
            continue
        if key.endswith(".#is_none"):
            continue  # drop all markers (true and false)
        if any(_is_under(root, key) for root in none_true_roots):
            continue  # drop the subtree of a None-folded option
        folded[key] = value
    for root in none_true_roots:
        if root.split(".", 1)[0] != "components":
            folded[root] = None  # the option itself folds to null
    return folded


def _eval_layer_jsonnet(layer_dir: Path) -> dict:
    """Evaluate `<layer>/sequencer_config.jsonnet` to a nested Python dict."""
    path = layer_dir / "sequencer_config.jsonnet"
    return json.loads(_jsonnet.evaluate_file(str(path), jpathdir=[str(JSONNET_DIR)]))


def _assert_layer_jsonnet_mirrors_combined_yaml(layer_dir: Path):
    """Shared assertion for the per-layer regression tests.

    Asserts the layer's `sequencer_config.jsonnet` (flattened to dotted keys) has EXACTLY the keys
    of, and equal values to, the combined `config.sequencerConfig` of that layer's YAMLs (folded:
    `#is_none` applied, `components.*` dropped). This keeps the native override layers in lockstep
    with the YAML overlays the preset path consumes, so the two generation paths cannot silently
    drift. (The storage-reader ports are not a special case: they are fixed infra constants, baked
    into the app_configs and `applicative_config.libsonnet`, and absent from BOTH the YAMLs and the
    jsonnet layers.)
    """
    jsonnet_flat = _flatten(_eval_layer_jsonnet(layer_dir))
    expected = _fold_is_none_drop_components(_combined_layer_sequencer_config(layer_dir))

    missing_in_jsonnet = sorted(set(expected) - set(jsonnet_flat))
    extra_in_jsonnet = sorted(set(jsonnet_flat) - set(expected))
    value_diffs = sorted(
        key for key in set(jsonnet_flat) & set(expected) if jsonnet_flat[key] != expected[key]
    )

    assert not (missing_in_jsonnet or extra_in_jsonnet or value_diffs), (
        f"{layer_dir.name}/sequencer_config.jsonnet diverges from the combined "
        f"config.sequencerConfig of its YAMLs (folded):\n"
        f"  missing in jsonnet ({len(missing_in_jsonnet)}): {missing_in_jsonnet}\n"
        f"  extra in jsonnet ({len(extra_in_jsonnet)}): {extra_in_jsonnet}\n"
        f"  value diffs ({len(value_diffs)}):\n    "
        + "\n    ".join(
            f"{key}: jsonnet={jsonnet_flat[key]!r} yaml={expected[key]!r}" for key in value_diffs
        )
    )


def test_common_layer_jsonnet_mirrors_combined_yaml():
    """REGRESSION: the common base `sequencer_config.jsonnet` equals the combined common-layer
    `config.sequencerConfig` (folded), exactly.

    Catches drift in either direction: a common-YAML override not transcribed into the base jsonnet,
    a stale/extra key in the jsonnet, or a value mismatch.
    """
    _assert_layer_jsonnet_mirrors_combined_yaml(COMMON_LAYER_DIR)


def test_integration_layer_jsonnet_mirrors_combined_yaml():
    """REGRESSION: same invariant for the `sepolia-integration` env layer."""
    _assert_layer_jsonnet_mirrors_combined_yaml(INTEGRATION_LAYER_DIR)


def test_sepolia_alpha_layer_jsonnet_mirrors_combined_yaml():
    """REGRESSION: same invariant for the `sepolia-alpha` env layer."""
    _assert_layer_jsonnet_mirrors_combined_yaml(SEPOLIA_ALPHA_LAYER_DIR)
