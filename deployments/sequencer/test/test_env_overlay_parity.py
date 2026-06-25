"""Parity test: each overlay's bucketed native config matches its YAML `config.sequencerConfig`.

Asserts every key set by the layer's bucketed jsonnet (chain_params + replacers, flattened) exists in
the layer's folded YAML sequencerConfig with an equal value. Covers the sepolia/mainnet env overlays
and the `testing/*` overlays (node-0, all-constructs).

Skips when the YAML `config.sequencerConfig` is absent (either removed downstack at
`drop-yaml-sequencer-config`, or a structure-only stub like `all-constructs` whose sole entry is a
folded-away `components.*` marker), so the test self-disables instead of failing on a missing source.
"""

import json
from pathlib import Path

import _jsonnet
import pytest
import yaml

from src.config.native import JSONNET_DIR

DEPLOYMENTS_SEQUENCER = Path(__file__).resolve().parents[1]
HYBRID_OVERLAYS_DIR = DEPLOYMENTS_SEQUENCER / "configs" / "overlays" / "hybrid"


def _flatten(nested: dict, prefix: str = "") -> dict:
    """Flatten a nested config to dotted keys. Lists and null are leaf values (not recursed)."""
    flat: dict = {}
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
    """Merge the flat-dotted `config.sequencerConfig` across one overlay layer's own YAMLs.

    `<layer>/common.yaml` first, then each `<layer>/services/*.yaml` (sorted, last wins). Does NOT
    expand `include:` — only the layer's own files. Still carries `.#is_none` markers and
    `components.*` (fold them with `_fold_is_none_drop_components`).
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
    """Apply the transform the jsonnet layers encode: `#is_none:true` -> null (drop the subtree),
    drop `#is_none:false` markers (keep the real leaves), drop `components.*`."""
    none_true_roots = [
        key[: -len(".#is_none")]
        for key, value in flat.items()
        if key.endswith(".#is_none") and value is True
    ]
    folded: dict = {}
    for key, value in flat.items():
        if key.split(".", 1)[0] == "components":
            continue
        if key.endswith(".#is_none"):
            continue
        if any(_is_under(root, key) for root in none_true_roots):
            continue
        folded[key] = value
    for root in none_true_roots:
        if root.split(".", 1)[0] != "components":
            folded[root] = None
    return folded


def _layer_bucketed_override_flat(layer_dir: Path) -> dict:
    """The layer's bucketed override (chain_params + replacers) as flat config-override dotted keys.

    node_params (validator_id / P2P multiaddrs) are supplied by the devops layers and absent from the
    env YAML, so they are out of scope and excluded.
    """
    flat: dict = {}
    for name in ("chain_params.jsonnet", "replacers.jsonnet"):
        path = layer_dir / name
        if not path.is_file():
            continue
        obj = json.loads(_jsonnet.evaluate_file(str(path), jpathdir=[str(JSONNET_DIR)]))
        flat.update(_flatten(obj))
    # chain_params exposes `starknet_contract_address` at the top level, but the applicative config
    # (and the YAML) place it under `base_layer_config`; remap so the parity comparison aligns.
    if "starknet_contract_address" in flat:
        flat["base_layer_config.starknet_contract_address"] = flat.pop("starknet_contract_address")
    return flat


def assert_env_overlay_matches_yaml(layer_dir: Path) -> None:
    """Assert every key the env layer's bucketed native config sets matches its YAML sequencerConfig.

    Skips if the YAML sequencerConfig is gone (removed downstack at `drop-yaml-sequencer-config`).
    """
    yaml_flat = _fold_is_none_drop_components(_combined_layer_sequencer_config(layer_dir))
    if not yaml_flat:
        pytest.skip(f"{layer_dir.name}: no YAML config.sequencerConfig (removed downstack)")

    jsonnet_flat = _layer_bucketed_override_flat(layer_dir)

    mismatches = []
    for key, value in sorted(jsonnet_flat.items()):
        if key not in yaml_flat:
            mismatches.append(f"{key}: present in jsonnet, missing from YAML")
        elif yaml_flat[key] != value:
            mismatches.append(f"{key}: jsonnet={value!r} != yaml={yaml_flat[key]!r}")

    assert not mismatches, (
        f"{layer_dir.name} native override diverges from its YAML config.sequencerConfig:\n  "
        + "\n  ".join(mismatches)
    )


def test_sepolia_integration_native_matches_yaml():
    assert_env_overlay_matches_yaml(HYBRID_OVERLAYS_DIR / "sepolia-integration")


def test_sepolia_alpha_native_matches_yaml():
    assert_env_overlay_matches_yaml(HYBRID_OVERLAYS_DIR / "sepolia-alpha")


def test_mainnet_native_matches_yaml():
    assert_env_overlay_matches_yaml(HYBRID_OVERLAYS_DIR / "mainnet")


def test_node_0_native_matches_yaml():
    assert_env_overlay_matches_yaml(HYBRID_OVERLAYS_DIR / "testing" / "node-0")


def test_all_constructs_native_matches_yaml():
    assert_env_overlay_matches_yaml(HYBRID_OVERLAYS_DIR / "testing" / "all-constructs")
