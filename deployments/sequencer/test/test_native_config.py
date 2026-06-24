"""Tests for the native (nested) node-config generation path (Phase C).

Covers:
  - the null-preserving deep-merge,
  - the overlay-service-name -> build-key mapping,
  - the AUTHORITATIVE PARITY test: native vs folded/filtered preset for integration node-0 `core`.
"""

import json
import os
import shutil
from pathlib import Path

import _jsonnet
import pytest
import yaml
from src.config.loaders import NodeConfigLoader
from src.config.native import (
    JSONNET_DIR,
    LAYER_GLOB,
    build_native_config,
    deep_merge_preserving_null,
    merged_overrides,
    resolve_layer_files,
    service_name_to_build_key,
)

DEPLOYMENTS_SEQUENCER = Path(__file__).resolve().parents[1]
REPO_ROOT = DEPLOYMENTS_SEQUENCER.parents[1]

LAYOUT = "hybrid"
# Overlay chain for integration node-0, matching the `-o` flags a real synth passes.
INTEGRATION_NODE0_OVERLAYS = [
    "hybrid.sepolia-integration",
    "hybrid.sepolia-integration.apollo-sepolia-integration-0",
]
# A real single-leaf-overlay deploy passes ONLY the leaf overlay (not its env ancestor); the native
# resolver must still visit the intermediate `sepolia-integration` env dir via ancestor expansion.
INTEGRATION_NODE0_LEAF_OVERLAY = ["hybrid.sepolia-integration.apollo-sepolia-integration-0"]
CORE_SERVICE = "core"
CORE_CONFIG_LIST = (
    "crates/apollo_deployments/resources/services/hybrid/replacer_deployment_core.json"
)

# Per-layer override-file dirs (this repo): the base/common layer and the sepolia-integration env
# layer. Each holds a `sequencer_config.jsonnet` that must mirror the combined `config.sequencerConfig`
# of the YAMLs in the same dir (see `_assert_layer_jsonnet_mirrors_combined_yaml`).
HYBRID_OVERLAYS_DIR = DEPLOYMENTS_SEQUENCER / "configs" / "overlays" / LAYOUT
COMMON_LAYER_DIR = HYBRID_OVERLAYS_DIR / "common"
INTEGRATION_LAYER_DIR = HYBRID_OVERLAYS_DIR / "sepolia-integration"
SEPOLIA_ALPHA_LAYER_DIR = HYBRID_OVERLAYS_DIR / "sepolia-alpha"
MAINNET_LAYER_DIR = HYBRID_OVERLAYS_DIR / "mainnet"

# The committed dump of Rust `private_parameters()` (the secrets): every non-pointer private param
# plus every pointer target pointed by a private param. These are filled at deploy from the
# ExternalSecret `secrets.json` (the 2nd `--config_file`), so BOTH generated configs leave them at
# pre-secret defaults/placeholders and their generated representation can legitimately differ.
PRIVATE_PARAMETERS_SCHEMA = "crates/apollo_node/resources/config_secrets_schema.json"

# The cross-repo devops checkout supplies the env (devops) and per-node overlay layers. At deploy
# time these files are laid into the public repo's `configs/overlays/<layout>` tree before synth; in
# the dev checkout they live in a sibling repo (override the path via SEQUENCER_DEVOPS_DIR).
DEVOPS_DIR = Path(
    os.environ.get(
        "SEQUENCER_DEVOPS_DIR",
        "/home/nimrod/workspace/sequencer-devops/sequencer",
    )
)
DEVOPS_OVERLAYS = DEVOPS_DIR / "configs" / "overlays" / LAYOUT / "sepolia-integration"

# Whole-tree overlay roots for the cross-repo collision guard (not scoped to one env).
PUBLIC_OVERLAYS_ROOT = DEPLOYMENTS_SEQUENCER / "configs" / "overlays"
DEVOPS_OVERLAYS_ROOT = DEVOPS_DIR / "configs" / "overlays"


@pytest.fixture(scope="module")
def combined_base_dir(tmp_path_factory) -> str:
    """Build a base_dir whose `configs/overlays/<layout>` tree merges the public repo's overlays
    with the cross-repo devops overlays (devops wins), reproducing the deploy-time layout.

    Both the preset YAML loader and the native jsonnet resolver are pointed here, so they see the
    full base < env < devops-env < per-node chain. Layout/service files and the replacer JSON are
    referenced via the public repo (config_base_dir/ROOT relative paths), so we symlink the public
    `configs/` and `crates/` into the temp root and overlay devops files on top.

    Skips the test if the devops checkout is not present.
    """
    if not DEVOPS_OVERLAYS.is_dir():
        pytest.skip(f"devops overlay checkout not found at {DEVOPS_OVERLAYS}")

    base = tmp_path_factory.mktemp("combined_overlays")

    # Copy the whole public deployments/sequencer/configs tree (it is small), then overlay devops.
    shutil.copytree(DEPLOYMENTS_SEQUENCER / "configs", base / "configs")
    devops_env = DEVOPS_OVERLAYS
    public_env = base / "configs" / "overlays" / LAYOUT / "sepolia-integration"
    public_env.mkdir(parents=True, exist_ok=True)
    shutil.copytree(devops_env, public_env, dirs_exist_ok=True)

    # Layout/service includes and the replacer config list resolve relative to the repo root
    # (config_base_dir for relative `include:` paths, ROOT_DIR for the configList). Symlink the repo
    # `crates/` so those resolve from the temp base too.
    os.symlink(REPO_ROOT / "crates", base / "crates")
    return str(base)


def test_deep_merge_preserves_explicit_null():
    """A later layer setting a key to null overwrites an earlier non-null value (not skipped)."""
    base = {"a": {"x": 1, "y": 2}, "b": "keep"}
    overlay = {"a": {"y": None}}
    merged = deep_merge_preserving_null(base, overlay)
    assert merged == {"a": {"x": 1, "y": None}, "b": "keep"}
    # The null is a real value, present in the result.
    assert "y" in merged["a"] and merged["a"]["y"] is None


def test_deep_merge_nested_objects():
    """Nested dicts merge key-by-key; disjoint keys from both sides survive."""
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
    assert base == {"a": {"x": 1}}
    assert overlay == {"a": {"y": 2}}


def test_service_name_to_build_key_sierracompiler():
    assert service_name_to_build_key("sierracompiler") == "sierra_compiler"


@pytest.mark.parametrize("name", ["core", "gateway", "l1", "mempool", "committer"])
def test_service_name_to_build_key_identity(name):
    assert service_name_to_build_key(name) == name


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


def _is_placeholder(value) -> bool:
    return isinstance(value, str) and value.startswith("$$$_") and value.endswith("_$$$")


def _private_parameters() -> list[str]:
    """The private-parameter (secret) paths: the committed dump of Rust `private_parameters()`.

    These are filled at deploy time from the ExternalSecret `secrets.json` (the 2nd `--config_file`),
    so both the preset and the native generated config leave them at pre-secret defaults/placeholders
    whose representation can differ. Excluded from the parity comparison for that reason only.
    """
    return list(json.load(open(REPO_ROOT / PRIVATE_PARAMETERS_SCHEMA)))


def _non_default_paths(config_list_path: str) -> set[str]:
    """Derive the closure mirroring Rust `non_default_paths()` (jsonnet.rs:221).

    Computed from the UN-overridden replacer config (the same source the Rust test reads):
      - every flat dotted key whose replacer value is a `$$$_..._$$$` placeholder is an
        overridable/pointer/secret path (= KEYS_TO_BE_REPLACED + CONFIG_POINTERS pointing-paths +
        private_parameters);
      - for every `<path>.#is_none` marker, the option root `<path>` (the whole optional folds to
        the override, so the subtree is non-default).

    Used here NOT as the parity exclusion (that would drop the overridden values the migration must
    preserve, e.g. n_workers / ports / committee / multiaddrs), but only to validate that any
    native-only emitted key (a key the preset path never materializes) is a genuine pointer
    target / `#is_none`-folded option root rather than a stray native-side key.
    """
    config_list = json.load(open(REPO_ROOT / config_list_path))
    merged = {}
    for relative_path in config_list:
        merged.update(json.load(open(REPO_ROOT / relative_path)))

    paths = set()
    for key, value in merged.items():
        if _is_placeholder(value):
            paths.add(key)
        if key.endswith(".#is_none"):
            paths.add(key[: -len(".#is_none")])
    return paths


def _is_under(prefix: str, dotted_key: str) -> bool:
    """True if `dotted_key` equals `prefix` or is nested under it (segment-aligned)."""
    return dotted_key == prefix or dotted_key.startswith(prefix + ".")


def _fold_preset(preset_flat: dict) -> dict:
    """Fold the flat preset like the node's `update_optional_values`.

    For every `<path>.#is_none` marker:
      - if true: drop the `<path>.*` subtree (the option is None),
      - then drop the `.#is_none` marker itself either way.
    Returns a new flat dict with markers removed and None-subtrees dropped.
    """
    none_roots = [
        key[: -len(".#is_none")]
        for key, value in preset_flat.items()
        if key.endswith(".#is_none") and value is True
    ]
    folded = {}
    for key, value in preset_flat.items():
        if key.endswith(".#is_none"):
            continue  # drop all markers
        if any(_is_under(root, key) for root in none_roots):
            continue  # drop subtree of a None-folded option
        folded[key] = value
    return folded


def _build_preset_flat(base_dir: str) -> dict:
    """Build the integration node-0 `core` preset config (placeholder fill) as a flat dotted dict.

    Reproduces ConfigMapConstruct._build_preset_node_config for the merged sequencerConfig of the
    integration node-0 `core` service, resolved from the same overlay chain. `base_dir` is the
    combined (public + devops) overlay root, so the env/per-node devops YAML layers are visible.
    """
    from src.config.merger import merge_configs

    root = Path(base_dir)

    # Resolve the overlay layer (common + service) paths exactly as app._get_config_paths does.
    layout_services = root / "configs" / "layouts" / LAYOUT / "services"
    layout_common_path = root / "configs" / "layouts" / LAYOUT / "common.yaml"
    layout_common = str(layout_common_path) if layout_common_path.exists() else None

    overlay_layers = []
    for overlay in INTEGRATION_NODE0_OVERLAYS:
        segments = overlay.split(".")
        overlay_base = root / "configs" / "overlays" / LAYOUT
        for segment in segments[1:]:
            overlay_base = overlay_base / segment
        common_path = overlay_base / "common.yaml"
        services_path = overlay_base / "services"
        overlay_layers.append(
            (
                overlay,
                str(common_path) if common_path.exists() else None,
                str(services_path) if services_path.is_dir() else None,
            )
        )

    deployment_config = merge_configs(
        config_base_dir=base_dir,
        layout_common_config_path=layout_common,
        layout_services_config_dir_path=str(layout_services),
        overlay_layers=overlay_layers,
    )

    core_cfg = next(svc for svc in deployment_config.services if svc.name == CORE_SERVICE)
    loader = NodeConfigLoader(config_list_json_path=core_cfg.config.configList)
    node_config = loader.load()
    merged_sequencer_config = core_cfg.config.sequencerConfig or {}
    node_config = NodeConfigLoader.apply_sequencer_overrides(
        node_config,
        merged_sequencer_config,
        service_name=CORE_SERVICE,
        config_list_path=core_cfg.config.configList,
        layout=LAYOUT,
        overlays=INTEGRATION_NODE0_OVERLAYS,
    )
    # The preset output is already a flat dotted-key dict.
    return node_config


def _parity_applicative_views(
    preset_folded: dict, native_flat: dict, native_sections: set
) -> tuple:
    """Reduce both flat configs to the applicative keys the parity comparison covers.

    Excludes ONLY the genuine generation-time asymmetries:
      - `components.*`: live-DNS URLs/ports the layout supplies, baked in the native build but left
        as deploy-time placeholders in the preset (the preset's `components.*` are the k8s scaffold).
      - `private_parameters()` (the secrets): filled at deploy from the ExternalSecret `secrets.json`
        (2nd `--config_file`), so both sides leave them at pre-secret defaults whose representation
        can differ.
      - top-level `general_config` scalars the native build does NOT emit: it carries only the
        per-service component sections + `monitoring_config` + `components` + `validation_only`, so
        restrict to keys under a section the native build emitted (`native_sections`).

    Crucially it does NOT exclude the `non_default_paths()` closure: those overridable keys
    (n_workers, storage-reader ports, bouncer limits, multiaddrs, default_committee, timeouts,
    first_block_with_partial_block_hash, ...) are filled to the SAME integration values on both
    sides, so they MUST be compared — that is the whole point of the gate.

    Returns `(preset_applicative, native_applicative)` flat dicts.
    """
    excluded_prefixes = _private_parameters() + ["components"]

    def keep(dotted_key: str) -> bool:
        if dotted_key.split(".", 1)[0] not in native_sections:
            return False
        return not any(_is_under(prefix, dotted_key) for prefix in excluded_prefixes)

    preset_applicative = {k: v for k, v in preset_folded.items() if keep(k)}
    native_applicative = {k: v for k, v in native_flat.items() if keep(k)}
    return preset_applicative, native_applicative


def _parity_diffs(preset_applicative: dict, native_applicative: dict) -> tuple:
    """Compute (missing_in_native, missing_in_preset, value_diffs) between the two applicative views.

    `value_diffs` is a list of dotted keys whose value differs between the two sides.
    """
    missing_in_native = sorted(set(preset_applicative) - set(native_applicative))
    missing_in_preset = sorted(set(native_applicative) - set(preset_applicative))
    value_diffs = sorted(
        k
        for k in set(preset_applicative) & set(native_applicative)
        if preset_applicative[k] != native_applicative[k]
    )
    return missing_in_native, missing_in_preset, value_diffs


def test_native_matches_preset_parity_core_integration_node0(combined_base_dir):
    """AUTHORITATIVE PARITY: native vs folded/filtered preset for integration node-0 `core`.

    Both sides are reduced (see `_parity_applicative_views`) to the applicative keys, excluding only
    `components.*`, the `private_parameters()` secrets, and top-level general_config scalars the
    native build does not emit. The overridable/pointer keys (KEYS_TO_BE_REPLACED, CONFIG_POINTERS
    pointing-paths, `#is_none` roots) are NOT excluded: both sides are post-fill at the same
    integration values, so they must match — comparing them is the point of the gate. The compared
    set is therefore hundreds of keys (n_workers, storage-reader ports, bouncer limits, consensus
    multiaddrs, default_committee, timeouts, first_block_with_partial_block_hash, ...), not 3.

    Asserts, with zero tolerance:
      - no `value_diffs` on any key BOTH sides emit (a wrong overridden value is a migration bug),
      - no `missing_in_native` (a key the preset emits but the native build dropped is a bug).

    `missing_in_preset` (keys the native build emits but the preset path never materializes) is a
    legitimate, structural generation asymmetry: the native build bakes CONFIG_POINTERS targets into
    nested paths (e.g. `*.db_config.chain_id`, `chain_info.fee_token_addresses.*`, the cende
    `recorder_url`, the central-sync `starknet_url`) and emits `#is_none`-folded option roots as
    `null`, while the preset represents those as pointer placeholders the node fills at load time and
    drops the folded subtree. Each such native-only key is asserted to lie within
    `non_default_paths()` (the pointer-target / `#is_none`-root closure); any native-only key OUTSIDE
    that closure would be a stray native-side key and fails.
    """
    preset_folded = _fold_preset(_build_preset_flat(combined_base_dir))

    native_nested = build_native_config(
        service_name=CORE_SERVICE,
        layout=LAYOUT,
        overlays=INTEGRATION_NODE0_OVERLAYS,
        base_dir=combined_base_dir,
    )
    native_flat = _flatten(native_nested)
    native_sections = set(native_nested.keys())

    preset_applicative, native_applicative = _parity_applicative_views(
        preset_folded, native_flat, native_sections
    )

    # Prove the gate is non-trivial: the compared (both-emit) set must be in the hundreds, not ~3.
    compared_keys = set(preset_applicative) & set(native_applicative)
    print(f"parity compared keys (both sides emit): {len(compared_keys)}")
    assert len(compared_keys) > 100, (
        f"parity comparison covers only {len(compared_keys)} keys — the filter is too broad and the "
        "gate is meaningless; it must compare the overridable keys (n_workers, ports, committee, ...)"
    )

    missing_in_native, missing_in_preset, value_diffs = _parity_diffs(
        preset_applicative, native_applicative
    )

    # The native-only keys are allowed ONLY if they are genuine pointer-target / `#is_none`-root
    # asymmetries (in `non_default_paths()`); a native-only key outside that closure is a real bug.
    non_default = _non_default_paths(CORE_CONFIG_LIST)
    stray_native_only = sorted(
        key
        for key in missing_in_preset
        if not any(_is_under(prefix, key) for prefix in non_default)
    )

    value_diff_lines = [
        f"{k}: preset={preset_applicative[k]!r} native={native_applicative[k]!r}"
        for k in value_diffs
    ]
    assert not (missing_in_native or stray_native_only or value_diffs), (
        "native config diverges from folded/filtered preset:\n"
        f"  missing in native ({len(missing_in_native)}): {missing_in_native}\n"
        f"  stray native-only keys not in non_default_paths() ({len(stray_native_only)}): "
        f"{stray_native_only}\n"
        f"  value diffs ({len(value_diff_lines)}):\n    " + "\n    ".join(value_diff_lines)
    )


def test_parity_gate_catches_wrong_overridden_value(combined_base_dir):
    """NEGATIVE TEST: prove the parity gate gates.

    Take the real native config, mutate ONE overridden value (the batcher `n_workers`, which the
    integration overlay sets to 1 and which lives in the `non_default_paths()` closure the old filter
    wrongly excluded), and assert the parity comparison now reports a value difference. If the gate
    silently ignored this key (as the original 3-key filter did), this test would fail — so it proves
    the gate would catch a migration bug.
    """
    preset_folded = _fold_preset(_build_preset_flat(combined_base_dir))

    native_nested = build_native_config(
        service_name=CORE_SERVICE,
        layout=LAYOUT,
        overlays=INTEGRATION_NODE0_OVERLAYS,
        base_dir=combined_base_dir,
    )
    native_flat = _flatten(native_nested)
    native_sections = set(native_nested.keys())

    mutated_key = "batcher_config.static_config.block_builder_config.execute_config.n_workers"
    assert mutated_key in native_flat, "fixture changed: expected n_workers in the native config"
    # Corrupt the overridden value the way a buggy migration would (revert it to the app default 28).
    native_flat[mutated_key] = native_flat[mutated_key] + 27

    preset_applicative, native_applicative = _parity_applicative_views(
        preset_folded, native_flat, native_sections
    )
    # The mutated key must be in the compared (both-emit) set, else the gate cannot see it.
    assert mutated_key in (set(preset_applicative) & set(native_applicative))

    _, _, value_diffs = _parity_diffs(preset_applicative, native_applicative)
    assert (
        mutated_key in value_diffs
    ), "parity gate did NOT detect the corrupted overridden value — the gate does not gate"


def test_resolve_layer_files_single_leaf_visits_env_ancestor(combined_base_dir):
    """REGRESSION: a single leaf overlay must expand to its full ancestor chain (env dir included).

    A real single-leaf deploy passes only `hybrid.sepolia-integration.apollo-sepolia-integration-0`.
    The resolver must visit the intermediate `sepolia-integration` env dir (which carries
    `sequencer_config.jsonnet` + the devops `common_sequencer_config.jsonnet`), not just the leaf.
    The ordered file list must be exactly: base common, then the env dir's files (sorted), then the
    leaf — and must be IDENTICAL to passing the explicit env+leaf chain (shared ancestors deduped).
    """
    base = Path(combined_base_dir)
    overlays_root = base / "configs" / "overlays" / LAYOUT

    leaf_files = resolve_layer_files(LAYOUT, INTEGRATION_NODE0_LEAF_OVERLAY, base_dir=combined_base_dir)
    both_files = resolve_layer_files(LAYOUT, INTEGRATION_NODE0_OVERLAYS, base_dir=combined_base_dir)

    # Single-leaf and explicit env+leaf chain must yield the SAME ordered, deduped file list.
    assert leaf_files == both_files

    expected = (
        sorted((overlays_root / "common").glob(LAYER_GLOB))
        + sorted((overlays_root / "sepolia-integration").glob(LAYER_GLOB))
        + sorted(
            (overlays_root / "sepolia-integration" / "apollo-sepolia-integration-0").glob(LAYER_GLOB)
        )
    )
    assert leaf_files == expected

    # The env dir's files must be present exactly once (no double-merge), in root-to-leaf order.
    env_common = overlays_root / "sepolia-integration" / "common_sequencer_config.jsonnet"
    env_layer = overlays_root / "sepolia-integration" / "sequencer_config.jsonnet"
    assert leaf_files.count(env_common) == 1
    assert leaf_files.count(env_layer) == 1


def test_single_leaf_merged_overrides_includes_env_cache_size(combined_base_dir):
    """REGRESSION: the env-dir-only override (committer cache_size) is present from a single leaf.

    `committer_config.storage_config.cache_size` lives ONLY in the `sepolia-integration` env layer.
    Before the ancestor-expansion fix, a single leaf overlay skipped that dir, so the merged
    overrides lacked the key and `build()` hit `field does not exist: cache_size`. The single-leaf
    merged overrides must now contain it, and the single-leaf and env+leaf merges must be equal.
    """
    leaf_overrides = merged_overrides(LAYOUT, INTEGRATION_NODE0_LEAF_OVERLAY, base_dir=combined_base_dir)
    both_overrides = merged_overrides(LAYOUT, INTEGRATION_NODE0_OVERLAYS, base_dir=combined_base_dir)

    assert leaf_overrides == both_overrides
    assert "cache_size" in leaf_overrides["committer_config"]["storage_config"]


def test_single_leaf_native_core_equals_both_overlay(combined_base_dir):
    """REGRESSION: single-leaf native `core` build succeeds and equals the env+leaf build.

    This is the invocation that previously raised `field does not exist: cache_size`. It must now
    succeed and produce byte-for-byte the same nested config as passing the explicit env+leaf chain.
    """
    leaf_config = build_native_config(
        service_name=CORE_SERVICE,
        layout=LAYOUT,
        overlays=INTEGRATION_NODE0_LEAF_OVERLAY,
        base_dir=combined_base_dir,
    )
    both_config = build_native_config(
        service_name=CORE_SERVICE,
        layout=LAYOUT,
        overlays=INTEGRATION_NODE0_OVERLAYS,
        base_dir=combined_base_dir,
    )
    assert leaf_config == both_config


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


def test_mainnet_layer_jsonnet_mirrors_combined_yaml():
    """REGRESSION: same invariant for the `mainnet` env layer."""
    _assert_layer_jsonnet_mirrors_combined_yaml(MAINNET_LAYER_DIR)


def _relative_layer_files(overlays_root: Path) -> set[str]:
    """Every overlay file under `overlays_root`, as posix paths relative to that root."""
    return {
        path.relative_to(overlays_root).as_posix()
        for path in overlays_root.rglob("*")
        if path.is_file()
    }


def _leaf_keys(layer_file: Path) -> set[str]:
    """Evaluate a `*sequencer_config.jsonnet` layer file and flatten it to its dotted leaf paths."""
    rendered = _jsonnet.evaluate_file(str(layer_file), jpathdir=[str(JSONNET_DIR)])
    return set(_flatten(json.loads(rendered)))


@pytest.mark.skipif(
    not DEVOPS_OVERLAYS_ROOT.is_dir(),
    reason=f"devops overlay checkout not found at {DEVOPS_OVERLAYS_ROOT}",
)
def test_no_cross_repo_overlay_path_collision():
    """The deploy's `cp -rf devops/overlays -> public/configs/overlays` is a file-level REPLACE.

    Any overlay file authored at the SAME relative path in BOTH repos is silently clobbered (the
    bug this fix addresses: the devops env layer overwrote the public env layer). Assert the two
    overlay trees share NO relative path, so the copy is a pure union with nothing dropped.
    """
    public_relative_files = _relative_layer_files(PUBLIC_OVERLAYS_ROOT)
    devops_relative_files = _relative_layer_files(DEVOPS_OVERLAYS_ROOT)
    colliding_relative_files = sorted(public_relative_files & devops_relative_files)
    assert not colliding_relative_files, (
        "overlay files share a relative path across the public and devops repos; the deploy "
        "`cp -rf` would clobber the public copy with the devops copy (or vice versa). Rename one "
        f"side so the paths are distinct. Offenders ({len(colliding_relative_files)}):\n  "
        + "\n  ".join(colliding_relative_files)
    )


@pytest.mark.skipif(
    not DEVOPS_OVERLAYS_ROOT.is_dir(),
    reason=f"devops overlay checkout not found at {DEVOPS_OVERLAYS_ROOT}",
)
def test_colocated_layer_files_have_disjoint_leaf_keys(combined_base_dir):
    """Within each overlay dir of the deploy-merged tree, the `*sequencer_config.jsonnet` layer files
    must have DISJOINT leaf-key sets.

    `resolve_layer_files` globs every `*sequencer_config.jsonnet` in a dir and deep-merges them in
    sorted order. If two co-located layers set the same leaf key, the sorted-merge order would decide
    the winner — a silent, order-dependent override. Disjoint leaf keys make the merge order
    immaterial. The merged tree (public applicative + devops P2P, devops laid on top) is built by the
    `combined_base_dir` fixture, exactly reproducing the deploy-time layout.
    """
    merged_overlays_root = Path(combined_base_dir) / "configs" / "overlays"
    layer_dirs = {layer_file.parent for layer_file in merged_overlays_root.rglob(LAYER_GLOB)}

    overlapping_dirs = {}
    for layer_dir in sorted(layer_dirs):
        layer_files = sorted(layer_dir.glob(LAYER_GLOB))
        if len(layer_files) < 2:
            continue
        seen_keys: dict[str, Path] = {}
        dir_overlaps = []
        for layer_file in layer_files:
            for leaf_key in _leaf_keys(layer_file):
                if leaf_key in seen_keys:
                    dir_overlaps.append(
                        f"{leaf_key} (in {seen_keys[leaf_key].name} and {layer_file.name})"
                    )
                else:
                    seen_keys[leaf_key] = layer_file
        if dir_overlaps:
            relative_dir = layer_dir.relative_to(merged_overlays_root).as_posix()
            overlapping_dirs[relative_dir] = sorted(dir_overlaps)

    assert not overlapping_dirs, (
        "co-located `*sequencer_config.jsonnet` layer files share leaf keys, so the sorted-merge "
        "order silently decides the winner. Move the conflicting key to one layer only:\n"
        + "\n".join(
            f"  {relative_dir}:\n    " + "\n    ".join(overlaps)
            for relative_dir, overlaps in sorted(overlapping_dirs.items())
        )
    )
