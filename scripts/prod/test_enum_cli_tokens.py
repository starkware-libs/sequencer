#!/usr/bin/env python3
"""Tests that enum CLI tokens (used in argparse choices/help/errors) match accepted input."""

import argparse

import pytest
from common_lib import RestartStrategy, Service, restart_strategy_converter


def test_restart_strategy_str_is_the_accepted_token():
    assert str(RestartStrategy.ALL_AT_ONCE) == "all_at_once"
    assert str(RestartStrategy.ONE_BY_ONE) == "one_by_one"
    assert str(RestartStrategy.NO_RESTART) == "no_restart"
    # The string form round-trips back through the converter.
    for strategy in RestartStrategy:
        assert restart_strategy_converter(str(strategy)) is strategy


def test_service_str_is_the_accepted_token():
    assert str(Service.Core) == "Core"
    assert str(Service.SierraCompiler) == "SierraCompiler"
    # The string form round-trips back through name lookup (how --service is parsed).
    for service in Service:
        assert Service[str(service)] is service


def test_committer_service_maps_to_statefulset_resources():
    # Committer runs as a StatefulSet, so it follows the Core "-statefulset-0" pod pattern.
    assert Service.Committer.config_map_name == "sequencer-committer-config"
    assert Service.Committer.pod_name == "sequencer-committer-statefulset-0"


def test_invalid_restart_strategy_raises_informative_error():
    # Enum value lookup raises ValueError; the converter must translate it to ArgumentTypeError
    # with the valid options, rather than letting argparse fall back to a generic message.
    with pytest.raises(argparse.ArgumentTypeError) as error_info:
        restart_strategy_converter("bogus")
    message = str(error_info.value)
    assert "all_at_once" in message
    assert "one_by_one" in message
    assert "no_restart" in message


if __name__ == "__main__":
    raise SystemExit(pytest.main([__file__, "-v"]))
