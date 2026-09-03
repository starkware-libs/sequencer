#!/usr/bin/env python3
"""Nested-path overrides land where the node reads them; typos and disabled components are handled."""

import pytest
from set_node_revert_mode import REVERT_CONFIG_PATHS, revert_config_overrides
from update_config_and_restart_nodes_lib import ConstConfigValuesUpdater, set_nested_config_value


def _node_config() -> dict:
    return {
        "consensus_manager_config": {"revert_config": None, "network_config": {"port": 53080}},
        "state_sync_config": {
            "static_config": {"revert_config": None, "central_sync_client_config": None}
        },
    }


def test_revert_overrides_write_both_nested_copies():
    updated = ConstConfigValuesUpdater(
        revert_config_overrides(True, 1234)
    ).get_updated_config_for_instance(_node_config(), instance_index=0)
    assert updated["consensus_manager_config"]["revert_config"] == 1234
    assert updated["state_sync_config"]["static_config"]["revert_config"] == 1234
    assert set(revert_config_overrides(False, 2**64 - 1).values()) == {None}
    assert len(REVERT_CONFIG_PATHS) == 2


def test_override_does_not_mutate_the_input():
    original = _node_config()
    ConstConfigValuesUpdater(
        {"consensus_manager_config.revert_config": 7}
    ).get_updated_config_for_instance(original, instance_index=0)
    assert original["consensus_manager_config"]["revert_config"] is None


def test_disabled_component_is_skipped():
    config = _node_config()
    path = "state_sync_config.static_config.central_sync_client_config.central_source_config.starknet_url"
    assert set_nested_config_value(config, path, "http://x") is False
    assert config["state_sync_config"]["static_config"]["central_sync_client_config"] is None


@pytest.mark.parametrize(
    "path",
    ["revert_config", "consensus_manager_config.revert_configg", "consensus_manager_config.nope.x"],
)
def test_unknown_path_is_rejected(path):
    with pytest.raises(KeyError):
        set_nested_config_value(_node_config(), path, 1)
