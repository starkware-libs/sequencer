"""Tests for the native (nested) node-config generation machinery.

Covers the pure building blocks of the native config builder (`src/config/native.py`):
  - the null-preserving deep-merge.

The override-layer data (the per-layer `sequencer_config.jsonnet` files) and the data-validation
tests that exercise it end-to-end — the AUTHORITATIVE PARITY test (native vs folded/filtered preset)
and the "jsonnet mirrors combined YAML" layer tests — land together with those layers in a follow-up.
"""

from src.config.native import deep_merge_preserving_null


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
