#!/usr/bin/env python3
"""Unit tests for the parallel restart/wait orchestration in restarter_lib (no cluster needed).

Kubectl-level calls (`_restart_pod`, `_wait_for_pod_to_satisfy_condition`) are mocked; the tests
verify that ALL_AT_ONCE drives restarts and waits concurrently while ONE_BY_ONE / NO_RESTART stay
sequential.
"""

import signal
import threading
import time

import pytest
from common_lib import NamespaceAndInstructionArgs, RestartStrategy, Service
from metrics_lib import MetricConditionGater
from restarter_lib import ServiceRestarter, WaitOnMetricRestarter

NAMESPACES = ["ns-0", "ns-1", "ns-2", "ns-3"]
SLEEP_PER_CALL_SECONDS = 0.2


def _make_args() -> NamespaceAndInstructionArgs:
    return NamespaceAndInstructionArgs(NAMESPACES, cluster_list=None)


class _ConcurrencyRecorder:
    """Records calls and the peak number that ran concurrently."""

    def __init__(self):
        self._lock = threading.Lock()
        self.calls = []
        self.current = 0
        self.peak = 0

    def enter(self, key):
        with self._lock:
            self.calls.append(key)
            self.current += 1
            self.peak = max(self.peak, self.current)
        time.sleep(SLEEP_PER_CALL_SECONDS)
        with self._lock:
            self.current -= 1


def test_all_at_once_restarts_every_node_concurrently(monkeypatch):
    monkeypatch.setattr("restarter_lib.wait_until_y_or_n", lambda question: True)
    recorder = _ConcurrencyRecorder()
    monkeypatch.setattr(
        ServiceRestarter,
        "_restart_pod",
        staticmethod(lambda namespace, service, index, cluster=None: recorder.enter(namespace)),
    )

    restarter = ServiceRestarter.from_restart_strategy(
        RestartStrategy.ALL_AT_ONCE, _make_args(), Service.Core
    )
    assert restarter.parallel is True

    start = time.monotonic()
    restarter.restart_all(max_parallelism=len(NAMESPACES))
    elapsed = time.monotonic() - start

    assert sorted(recorder.calls) == sorted(NAMESPACES)
    assert recorder.peak == len(NAMESPACES)  # all ran at the same time
    assert elapsed < SLEEP_PER_CALL_SECONDS * len(NAMESPACES)  # not sequential


def test_max_parallelism_caps_concurrency(monkeypatch):
    monkeypatch.setattr("restarter_lib.wait_until_y_or_n", lambda question: True)
    recorder = _ConcurrencyRecorder()
    monkeypatch.setattr(
        ServiceRestarter,
        "_restart_pod",
        staticmethod(lambda namespace, service, index, cluster=None: recorder.enter(namespace)),
    )

    restarter = ServiceRestarter.from_restart_strategy(
        RestartStrategy.ALL_AT_ONCE, _make_args(), Service.Core
    )
    restarter.restart_all(max_parallelism=2)

    assert sorted(recorder.calls) == sorted(NAMESPACES)
    assert recorder.peak <= 2


def test_all_at_once_waits_for_every_node_concurrently(monkeypatch):
    monkeypatch.setattr("restarter_lib.wait_until_y_or_n", lambda question: True)
    # Restarts are mocked to be instant; the wait phase is what we measure.
    monkeypatch.setattr(
        ServiceRestarter, "_restart_pod", staticmethod(lambda *args, **kwargs: None)
    )
    # Avoid touching real signal handlers from the test's main thread.
    monkeypatch.setattr(signal, "signal", lambda *args, **kwargs: None)

    recorder = _ConcurrencyRecorder()

    def fake_wait(self, instance_index):
        recorder.enter(self.namespace_and_instruction_args.get_namespace(instance_index))
        return True

    monkeypatch.setattr(WaitOnMetricRestarter, "_wait_for_pod_to_satisfy_condition", fake_wait)

    restarter = WaitOnMetricRestarter(
        _make_args(),
        Service.Core,
        [MetricConditionGater.Metric("some_metric", lambda value: True)],
        8082,
        RestartStrategy.ALL_AT_ONCE,
    )
    assert restarter.parallel is True

    start = time.monotonic()
    restarter.restart_all(max_parallelism=len(NAMESPACES))
    elapsed = time.monotonic() - start

    assert sorted(recorder.calls) == sorted(NAMESPACES)
    assert recorder.peak == len(NAMESPACES)
    assert elapsed < SLEEP_PER_CALL_SECONDS * len(NAMESPACES)


def test_one_by_one_is_sequential():
    restarter = ServiceRestarter.from_restart_strategy(
        RestartStrategy.ONE_BY_ONE, _make_args(), Service.Core
    )
    assert restarter.parallel is False


def test_no_restart_metric_restarter_is_sequential():
    restarter = WaitOnMetricRestarter(
        _make_args(),
        Service.Core,
        [MetricConditionGater.Metric("some_metric", lambda value: True)],
        8082,
        RestartStrategy.NO_RESTART,
    )
    assert restarter.parallel is False


def test_core_all_at_once_aborts_when_user_declines(monkeypatch):
    monkeypatch.setattr("restarter_lib.wait_until_y_or_n", lambda question: False)
    restarted = []
    monkeypatch.setattr(
        ServiceRestarter,
        "_restart_pod",
        staticmethod(lambda namespace, service, index, cluster=None: restarted.append(namespace)),
    )

    restarter = ServiceRestarter.from_restart_strategy(
        RestartStrategy.ALL_AT_ONCE, _make_args(), Service.Core
    )
    with pytest.raises(SystemExit) as exit_info:
        restarter.restart_all(max_parallelism=len(NAMESPACES))

    assert exit_info.value.code == 1
    assert restarted == []  # declining means nothing is restarted


def test_non_core_all_at_once_does_not_prompt(monkeypatch):
    def fail_if_called(question):
        raise AssertionError("Non-Core restarts must not prompt for confirmation")

    monkeypatch.setattr("restarter_lib.wait_until_y_or_n", fail_if_called)
    restarted = []
    monkeypatch.setattr(
        ServiceRestarter,
        "_restart_pod",
        staticmethod(lambda namespace, service, index, cluster=None: restarted.append(namespace)),
    )

    restarter = ServiceRestarter.from_restart_strategy(
        RestartStrategy.ALL_AT_ONCE, _make_args(), Service.Gateway
    )
    restarter.restart_all(max_parallelism=len(NAMESPACES))

    assert sorted(restarted) == sorted(NAMESPACES)


if __name__ == "__main__":
    raise SystemExit(pytest.main([__file__, "-v"]))
