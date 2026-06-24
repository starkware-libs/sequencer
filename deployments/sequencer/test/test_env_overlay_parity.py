"""Parity test: each env overlay's bucketed native config matches its YAML `config.sequencerConfig`.

Asserts every key set by the layer's bucketed jsonnet (chain_params, flattened) exists in
the layer's folded YAML sequencerConfig with an equal value.

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

_FLAT_TO_CONFIG_PATH = {
    "starknet_contract_address": "base_layer_config.starknet_contract_address",
    "base_layer": "base_layer_config",
    "staking_default_committee": "consensus_manager_config.staking_manager_config.dynamic_config.default_committee",
    "proof_archive_bucket_name": "gateway_config.static_config.proof_archive_writer_config.bucket_name",
    "consensus_advertised_multiaddr": "consensus_manager_config.network_config.advertised_multiaddr",
    "consensus_bootstrap_peer_multiaddr": "consensus_manager_config.network_config.bootstrap_peer_multiaddr",
    "mempool_advertised_multiaddr": "mempool_p2p_config.network_config.advertised_multiaddr",
    "mempool_bootstrap_peer_multiaddr": "mempool_p2p_config.network_config.bootstrap_peer_multiaddr",
    "n_concurrent_txs": "batcher_config.dynamic_config.n_concurrent_txs",
    "proposer_idle_detection_delay_millis": "batcher_config.dynamic_config.proposer_idle_detection_delay_millis",
    "max_events_in_block": "batcher_config.static_config.block_builder_config.bouncer_config.block_max_capacity.n_events",
    "max_receipt_l2_gas_in_block": "batcher_config.static_config.block_builder_config.bouncer_config.block_max_capacity.receipt_l2_gas",
    "max_state_diff_in_block": "batcher_config.static_config.block_builder_config.bouncer_config.block_max_capacity.state_diff_size",
    "n_execution_workers": "batcher_config.static_config.block_builder_config.execute_config.n_workers",
    "first_block_with_partial_block_hash": "batcher_config.static_config.first_block_with_partial_block_hash",
    "committer_cache_size": "committer_config.storage_config.cache_size",
    "committer_inner_storage_cache_size": "committer_config.storage_config.inner_storage_config.cache_size",
    "proposal_timeout_base": "consensus_manager_config.consensus_manager_config.dynamic_config.timeouts.proposal.base",
    "proposal_timeout_max": "consensus_manager_config.consensus_manager_config.dynamic_config.timeouts.proposal.max",
    "min_l2_gas_price_per_height": "consensus_manager_config.context_config.dynamic_config.min_l2_gas_price_per_height",
    "override_eth_to_fri_rate": "consensus_manager_config.context_config.dynamic_config.override_eth_to_fri_rate",
    "override_l1_data_gas_price_fri": "consensus_manager_config.context_config.dynamic_config.override_l1_data_gas_price_fri",
    "override_l1_gas_price_fri": "consensus_manager_config.context_config.dynamic_config.override_l1_gas_price_fri",
    "override_l2_gas_price_fri": "consensus_manager_config.context_config.dynamic_config.override_l2_gas_price_fri",
    "authorized_declarer_accounts": "gateway_config.static_config.authorized_declarer_accounts",
    "max_allowed_nonce_gap": "gateway_config.static_config.stateful_tx_validator_config.max_allowed_nonce_gap",
    "max_contract_bytecode_size": "gateway_config.static_config.stateless_tx_validator_config.max_contract_bytecode_size",
    "min_gas_price": "gateway_config.static_config.stateless_tx_validator_config.min_gas_price",
    "transaction_ttl": "mempool_config.dynamic_config.transaction_ttl",
    "audited_libfuncs_only": "sierra_compiler_config.audited_libfuncs_only",
    "max_bytecode_size": "sierra_compiler_config.max_bytecode_size",
    "central_sync_client_config": "state_sync_config.static_config.central_sync_client_config",
    "state_sync_network_config": "state_sync_config.static_config.network_config",
    "p2p_sync_client_config": "state_sync_config.static_config.p2p_sync_client_config",
}


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
    """The layer's `chain_params` override as flat config-override dotted keys, expanded to the full
    config paths (via `_FLAT_TO_CONFIG_PATH`) that the YAML `config.sequencerConfig` uses.

    node_params (validator_id / P2P multiaddrs) are supplied by the devops layers and absent from the
    env YAML, so they are out of scope and excluded.
    """
    path = layer_dir / "chain_params.jsonnet"
    if not path.is_file():
        return {}
    flat = _flatten(json.loads(_jsonnet.evaluate_file(str(path), jpathdir=[str(JSONNET_DIR)])))
    # Expand each flat name to its config path, preserving any sub-path for object-valued entries
    # (e.g. `first_block_with_partial_block_hash.block_hash`); unmapped names pass through unchanged.
    expanded: dict = {}
    for key, value in flat.items():
        head, _, rest = key.partition(".")
        full = _FLAT_TO_CONFIG_PATH.get(head, head)
        expanded[f"{full}.{rest}" if rest else full] = value
    return expanded


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
