#!/usr/bin/env python3
"""Unit tests for `run_in_parallel` in common_lib (no kubectl / cluster needed)."""

import time

import pytest
from common_lib import print_colored, run_in_parallel


def _label(item) -> str:
    return f"node-{item}"


def test_results_match_input_order_regardless_of_completion_order():
    # Earlier items sleep longer, so they finish last — results must still be in input order.
    def worker(item: int) -> int:
        time.sleep((5 - item) * 0.02)
        return item * 10

    results = run_in_parallel([0, 1, 2, 3, 4], worker, max_parallelism=4, label=_label)
    assert results == [0, 10, 20, 30, 40]


def test_empty_items_returns_empty_list():
    calls = []
    results = run_in_parallel([], lambda item: calls.append(item), max_parallelism=4, label=_label)
    assert results == []
    assert calls == []


def test_worker_output_is_buffered_grouped_and_labeled(capsys):
    def worker(item: int) -> None:
        print_colored(f"line-a from {item}")
        print_colored(f"line-b from {item}")

    run_in_parallel([0, 1], worker, max_parallelism=2, label=_label)
    out = capsys.readouterr().out

    # Each node's lines are flushed contiguously after its own header (no interleaving between
    # nodes), even though both ran concurrently.
    for item in (0, 1):
        header_pos = out.index(f"node-{item}")
        line_a_pos = out.index(f"line-a from {item}")
        line_b_pos = out.index(f"line-b from {item}")
        assert header_pos < line_a_pos < line_b_pos
        # Nothing from the other node appears between this node's two lines.
        other = 1 - item
        assert f"from {other}" not in out[line_a_pos:line_b_pos]


def test_heartbeat_lists_still_running_items(capsys):
    # One slow item keeps the pool busy long enough for at least one heartbeat (interval 1s).
    def worker(item: int) -> int:
        if item == 0:
            time.sleep(2.5)
        return item

    run_in_parallel([0, 1], worker, max_parallelism=2, label=_label, heartbeat_interval_seconds=1)
    out = capsys.readouterr().out
    assert "still waiting on: node-0" in out
    assert "done]" in out


def test_failing_worker_is_reported_and_exits_nonzero(capsys):
    def worker(item: int) -> int:
        if item == 1:
            raise ValueError("boom from 1")
        return item

    with pytest.raises(SystemExit) as exit_info:
        run_in_parallel([0, 1, 2], worker, max_parallelism=3, label=_label)

    assert exit_info.value.code == 1
    err = capsys.readouterr().err
    assert "1 of 3 parallel operation(s) failed" in err
    assert "node-1: boom from 1" in err


if __name__ == "__main__":
    raise SystemExit(pytest.main([__file__, "-v"]))
