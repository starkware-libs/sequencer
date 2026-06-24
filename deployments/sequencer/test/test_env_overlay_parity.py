"""Parity tests: each env overlay's bucketed native config matches its YAML `config.sequencerConfig`.

For an env layer, asserts:
  (a) every key in the bucketed jsonnet (chain_params + replacers, flattened) exists in the layer's
      folded YAML sequencerConfig with an equal value; and
  (b) every YAML key NOT in the jsonnet equals the applicative default (i.e. it was correctly dropped
      by slimming because its value matched the schema default).

Skips when the YAML `config.sequencerConfig` has been removed downstack (at `drop-yaml-sequencer-config`),
so the test self-disables past that point instead of failing on a missing comparison source.
"""

import json
from pathlib import Path

import _jsonnet
import pytest
import yaml

from src.config.native import JSONNET_DIR

DEPLOYMENTS_SEQUENCER = Path(__file__).resolve().parents[1]
HYBRID_OVERLAYS_DIR = DEPLOYMENTS_SEQUENCER / "configs" / "overlays" / "hybrid"

# Applicative defaults for the cross-cutting replacer keys. These map to multiple config paths, so
# they have no single path in the built config to read a default from; kept in sync with
# `constants.libsonnet` / `applicative_config.libsonnet`. (None of the current envs set these in
# their own YAML, so this is a forward-looking guard.)
CROSS_CUTTING_DEFAULTS = {
    "eth_fee_token_address": "0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7",
    "strk_fee_token_address": "0x04718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d",
    "versioned_constants_overrides": None,
}

# Complete dummy params for computing the applicative replacer defaults. `build()` reads every
# chain_params/node_params field directly (a missing one errors), so all must be present here. This
# is NOT an env's `chain_params.jsonnet` (those omit the devops-supplied P2P `network_config` blocks).
# Keep in sync with `grep -nE 'chain_params\.|node_params\.' applicative_config.libsonnet`.
_DEFAULTS_STUB_PARAMS = {
    "chain_params": {
        "chain_id": "SN_STUB",
        "starknet_url": "http://stub/",
        "recorder_url": "http://stub/",
        "native_classes_whitelist": "All",
        "base_layer_config": {
            "bpo1_start_block_number": 0,
            "bpo2_start_block_number": 0,
            "fusaka_no_bpo_start_block_number": 0,
            "starknet_contract_address": "0x0",
        },
        "batcher_config": {"static_config": {"first_block_with_partial_block_hash": None}},
        "consensus_manager_config": {
            "network_config": {"advertised_multiaddr": None, "bootstrap_peer_multiaddr": None},
            "staking_manager_config": {"dynamic_config": {"default_committee": "0,10:"}},
        },
        "mempool_p2p_config": {
            "network_config": {"advertised_multiaddr": None, "bootstrap_peer_multiaddr": None}
        },
        "gateway_config": {"static_config": {"proof_archive_writer_config": {"bucket_name": ""}}},
    },
    "node_params": {"validator_id": "0x0"},
    "replacers": {},
}

_ABSENT = object()


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


def _eval_build_params(layout: str, params: dict) -> dict:
    """Evaluate `build(layout, params)` to JSON (used to compute defaults with empty replacers)."""
    snippet = "(import 'lib/build.libsonnet').build(%s, %s)" % (
        json.dumps(layout),
        json.dumps(params),
    )
    return json.loads(
        _jsonnet.evaluate_snippet("build_defaults", snippet, jpathdir=[str(JSONNET_DIR)])
    )


def _applicative_defaults_flat() -> dict:
    """The applicative replacer defaults as flat dotted keys: build the consolidated `node` service
    (which carries every component section) with empty replacers, then flatten."""
    built = _eval_build_params("consolidated", _DEFAULTS_STUB_PARAMS)
    return _flatten(built["node"])


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
    return flat


def assert_env_overlay_matches_yaml(layer_dir: Path) -> None:
    """Assert the env layer's bucketed native config is consistent with its YAML sequencerConfig.

    (a) every bucketed-jsonnet key is present in the folded YAML with an equal value;
    (b) every YAML key absent from the jsonnet equals the applicative default (correctly slimmed).
    Skips if the YAML sequencerConfig is gone (removed downstack at `drop-yaml-sequencer-config`).
    """
    yaml_flat = _fold_is_none_drop_components(_combined_layer_sequencer_config(layer_dir))
    if not yaml_flat:
        pytest.skip(f"{layer_dir.name}: no YAML config.sequencerConfig (removed downstack)")

    jsonnet_flat = _layer_bucketed_override_flat(layer_dir)
    defaults_flat = _applicative_defaults_flat()

    mismatches = []
    # (a) every key the jsonnet layer sets must match the YAML.
    for key, value in sorted(jsonnet_flat.items()):
        if key not in yaml_flat:
            mismatches.append(f"(a) {key}: present in jsonnet, missing from YAML")
        elif yaml_flat[key] != value:
            mismatches.append(f"(a) {key}: jsonnet={value!r} != yaml={yaml_flat[key]!r}")
    # (b) every YAML key the jsonnet dropped must equal the applicative default.
    for key, yaml_value in sorted(yaml_flat.items()):
        if key in jsonnet_flat:
            continue
        default = CROSS_CUTTING_DEFAULTS.get(key, defaults_flat.get(key, _ABSENT))
        if default is _ABSENT:
            mismatches.append(
                f"(b) {key}={yaml_value!r}: dropped from jsonnet but no known default"
            )
        elif yaml_value != default:
            mismatches.append(f"(b) {key}: yaml={yaml_value!r} != default={default!r}")

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
