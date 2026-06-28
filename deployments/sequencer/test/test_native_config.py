"""Tests for the native (nested) node-config generation path.

Covers:
  - the null-preserving deep-merge,
  - native synth of the structure-validation `all-constructs` overlay.
"""

from src.config.native import build_native_config, deep_merge_preserving_null

LAYOUT = "hybrid"
CORE_SERVICE = "core"
# Structure-validation overlay (cdk8s `kubectl validate` only); its native layer is a synth-only stub
# carrying the minimum dummy `overrides.*` values `build()` reads unconditionally.
ALL_CONSTRUCTS_OVERLAYS = ["hybrid.testing.all-constructs"]


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


def test_all_constructs_native_config_synthesizes():
    """REGRESSION: the `testing/all-constructs` overlay synthesizes a native config.

    `all-constructs` is a STRUCTURE-validation stub: its cdk8s output is only `kubectl validate`d for
    manifest structure, never for config content. Its native layer supplies the minimum dummy
    `overrides.*` values `build()` reads unconditionally. The only invariant we assert is that native
    synth SUCCEEDS and yields a nested config (the CI `sequencer_cdk8s-test.yml` job synths this
    overlay under `--config-format native`).
    """
    native_nested = build_native_config(
        service_name=CORE_SERVICE,
        layout=LAYOUT,
        overlays=ALL_CONSTRUCTS_OVERLAYS,
    )
    assert isinstance(native_nested, dict) and native_nested
    # A nested SequencerNodeConfig (not the flat dotted preset form): top-level component sections.
    assert any(isinstance(value, dict) for value in native_nested.values())
