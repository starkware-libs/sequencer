"""Tests for the native (nested) node-config generation machinery.

Covers that the `testing/all-constructs` overlay synthesizes a native config end-to-end via
`src/config/native.py`'s `build_native_config`.
"""

from src.config.native import build_native_config

LAYOUT = "hybrid"
CORE_SERVICE = "core"
ALL_CONSTRUCTS_OVERLAYS = ["hybrid.testing.all-constructs"]


def test_all_constructs_native_config_synthesizes():
    """REGRESSION: the `testing/all-constructs` overlay synthesizes a native config.

    `all-constructs` is a STRUCTURE-validation stub: its cdk8s output is only `kubectl validate`d for
    manifest structure, never for config content. Its native override layers supply the minimum dummy
    chain_params/node_params (and a few replacer deltas) that `build()` needs. The only invariant we
    assert is that native synth SUCCEEDS and yields a nested config (the CI `sequencer_cdk8s-test.yml`
    job synths this overlay under `--config-format native`).
    """
    native_nested = build_native_config(
        service_name=CORE_SERVICE,
        layout=LAYOUT,
        overlays=ALL_CONSTRUCTS_OVERLAYS,
    )
    assert isinstance(native_nested, dict) and native_nested
    # A nested SequencerNodeConfig (not the flat dotted preset form): top-level component sections.
    assert any(isinstance(value, dict) for value in native_nested.values())
