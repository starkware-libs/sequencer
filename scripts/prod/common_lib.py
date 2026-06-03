#!/usr/bin/env python3

import argparse
import concurrent.futures
import subprocess
import sys
import threading
import time
from enum import Enum
from typing import Callable, Optional, TypeVar


class Colors(Enum):
    """ANSI color codes for terminal output"""

    RED = "\033[1;31m"
    GREEN = "\033[1;32m"
    YELLOW = "\033[1;33m"
    BLUE = "\033[1;34m"
    RESET = "\033[0m"


# Thread-local output sink. When `run_in_parallel` runs a worker, it sets `buffer` on this so that
# the worker's log lines are captured per-thread instead of interleaving on the shared stdout/stderr.
# When no buffer is set (the common, single-threaded case), logging prints immediately as before.
_output_sink = threading.local()

# Serializes writes to the real stdout/stderr so buffered blocks and heartbeats don't interleave.
_print_lock = threading.Lock()


def print_colored(message: str, color: Colors = Colors.RESET, file=None) -> None:
    """Print message with color.

    `file` is resolved to the current `sys.stdout` when None (resolving at call time rather than
    binding a default at definition time, so output redirection is honored).

    If the current thread has a buffer set on `_output_sink` (i.e. it is a `run_in_parallel`
    worker), the formatted line is appended to that buffer instead of being printed, so it can be
    flushed as one grouped block when the worker finishes.
    """
    if file is None:
        file = sys.stdout
    formatted = f"{color.value}{message}{Colors.RESET.value}"
    buffer = getattr(_output_sink, "buffer", None)
    if buffer is not None:
        buffer.append((formatted, file))
    else:
        print(formatted, file=file)


def print_error(message: str) -> None:
    print_colored(message, color=Colors.RED, file=sys.stderr)


T = TypeVar("T")
R = TypeVar("R")


def run_in_parallel(
    items: list[T],
    worker: Callable[[T], R],
    max_parallelism: int,
    label: Callable[[T], str],
    heartbeat_interval_seconds: int = 5,
) -> list[R]:
    """Run `worker(item)` for each item concurrently, capped at `max_parallelism` threads.

    Threads (not processes) are used because the work is I/O-bound (kubectl/urllib calls that
    release the GIL).

    Output: each worker's log lines (emitted via `print_colored`/`print_error`) are buffered and
    flushed as one block, prefixed with `label(item)`, when that item finishes — so concurrent
    output stays readable. While items are still running, a heartbeat naming the not-yet-done items
    is printed every `heartbeat_interval_seconds`.

    Errors: a worker that raises (or calls `sys.exit()`, which raises `SystemExit`) is recorded as
    a failure for its item; remaining items still run, and once all have settled a summary is
    printed and the process exits with code 1. `KeyboardInterrupt` is not treated as an item
    failure — it propagates so Ctrl-C aborts the whole run.

    Returns the per-item results in the same order as `items`.
    """
    if not items:
        return []

    num_items = len(items)
    results: list[Optional[R]] = [None] * num_items
    errors: dict[int, BaseException] = {}

    def run_one(item: T) -> R:
        buffer: list[tuple[str, object]] = []
        _output_sink.buffer = buffer
        try:
            return worker(item)
        finally:
            # Stop capturing before flushing so the header itself prints to the real stdout.
            _output_sink.buffer = None
            with _print_lock:
                print_colored(f"===== {label(item)} =====", Colors.BLUE)
                for text, file in buffer:
                    print(text, file=file)

    with concurrent.futures.ThreadPoolExecutor(
        max_workers=min(max_parallelism, num_items)
    ) as executor:
        future_to_index = {
            executor.submit(run_one, item): index for index, item in enumerate(items)
        }
        pending_futures = set(future_to_index.keys())
        last_heartbeat = time.monotonic()

        while pending_futures:
            done_futures, pending_futures = concurrent.futures.wait(
                pending_futures,
                timeout=heartbeat_interval_seconds,
                return_when=concurrent.futures.FIRST_COMPLETED,
            )
            for future in done_futures:
                index = future_to_index[future]
                try:
                    results[index] = future.result()
                except KeyboardInterrupt:
                    # Ctrl-C is not an item failure; let it abort the whole run.
                    raise
                except BaseException as error:
                    errors[index] = error

            now = time.monotonic()
            if pending_futures and now - last_heartbeat >= heartbeat_interval_seconds:
                running_labels = ", ".join(
                    label(items[future_to_index[future]]) for future in pending_futures
                )
                num_done = num_items - len(pending_futures)
                with _print_lock:
                    print_colored(
                        f"[{num_done}/{num_items} done] still waiting on: {running_labels}",
                        Colors.YELLOW,
                    )
                last_heartbeat = now

    if errors:
        with _print_lock:
            print_error(f"{len(errors)} of {num_items} parallel operation(s) failed:")
            for index in sorted(errors):
                print_error(f"  - {label(items[index])}: {errors[index]}")
        sys.exit(1)

    return results


class RestartStrategy(Enum):
    """Strategy for restarting nodes."""

    ALL_AT_ONCE = "all_at_once"
    ONE_BY_ONE = "one_by_one"
    NO_RESTART = "no_restart"

    def __str__(self) -> str:
        # The accepted CLI token is the value (e.g. "all_at_once"); use it so argparse choices and
        # error messages show what the user actually types rather than "RestartStrategy.ALL_AT_ONCE".
        return self.value


def restart_strategy_converter(strategy_name: str) -> RestartStrategy:
    """Convert string to RestartStrategy enum with informative error message"""
    RESTART_STRATEGY_PREFIX = f"{RestartStrategy.__name__}."
    if strategy_name.startswith(RESTART_STRATEGY_PREFIX):
        strategy_name = strategy_name[len(RESTART_STRATEGY_PREFIX) :]

    strategy_name = strategy_name.lower()

    try:
        # Looking an Enum up by value raises ValueError (not KeyError) when no member matches.
        return RestartStrategy(strategy_name)
    except ValueError:
        valid_strategies = ", ".join([strategy.value for strategy in RestartStrategy])
        raise argparse.ArgumentTypeError(
            f"Invalid restart strategy '{strategy_name}'. Valid options are: {valid_strategies}"
        )


class Service(Enum):
    """Service types mapping to their configmap and pod names."""

    Core = ("sequencer-core-config", "sequencer-core-statefulset-0")
    # Committer runs as a StatefulSet (like Core), so its pod ends in "-statefulset-0".
    Committer = ("sequencer-committer-config", "sequencer-committer-statefulset-0")
    Gateway = ("sequencer-gateway-config", "sequencer-gateway-deployment")
    L1 = ("sequencer-l1-config", "sequencer-l1-deployment")
    Mempool = ("sequencer-mempool-config", "sequencer-mempool-deployment")
    SierraCompiler = (
        "sequencer-sierracompiler-config",
        "sequencer-sierracompiler-deployment",
    )

    def __init__(self, config_map_name: str, pod_name: str) -> None:
        self.config_map_name = config_map_name
        self.pod_name = pod_name

    def __str__(self) -> str:
        # The accepted CLI token is the member name (e.g. "Core"); use it so argparse choices and
        # error messages show what the user actually types rather than "Service.Core".
        return self.name


class NamespaceAndInstructionArgs:
    def __init__(
        self,
        namespace_list: list[str],
        cluster_list: Optional[list[str]],
        instruction_list: Optional[list[str]] = None,
    ):
        assert (
            namespace_list is not None and len(namespace_list) > 0
        ), "Namespace list cannot be None or empty."
        self.namespace_list = namespace_list
        assert cluster_list is None or len(cluster_list) == len(
            namespace_list
        ), "cluster_list must have the same length as namespace_list"
        self.cluster_list = cluster_list
        assert instruction_list is None or len(namespace_list) == len(
            instruction_list
        ), "instruction_list must have the same length as namespace_list"
        self.instruction_list = instruction_list

    def size(self) -> int:
        return len(self.namespace_list)

    def get_namespace(self, index: int) -> str:
        return self.namespace_list[index]

    def get_cluster(self, index: int) -> Optional[str]:
        return self.cluster_list[index] if self.cluster_list is not None else None

    def get_instruction(self, index: int) -> Optional[str]:
        return self.instruction_list[index] if self.instruction_list is not None else None

    @staticmethod
    def get_namespace_list_from_args(
        args: argparse.Namespace,
    ) -> list[str]:
        """Get a list of namespaces based on the arguments"""
        if args.namespace_list:
            return args.namespace_list

        return [
            f"{args.namespace_prefix}-{i}"
            for i in range(args.start_index, args.start_index + args.num_nodes)
        ]

    @staticmethod
    def get_context_list_from_args(
        args: argparse.Namespace,
    ) -> list[str]:
        """Get a list of contexts based on the arguments"""
        if args.cluster_list:
            return args.cluster_list

        if args.cluster_prefix is None:
            return None

        return [
            f"{args.cluster_prefix}-{i}"
            for i in range(args.start_index, args.start_index + args.num_nodes)
        ]


def get_namespace_args(namespace: str, cluster: Optional[str] = None) -> list[str]:
    ret = ["-n", f"{namespace}"]
    if cluster:
        ret.extend(["--context", f"{cluster}"])
    return ret


def run_kubectl_command(args: list, capture_output: bool = True) -> subprocess.CompletedProcess:
    full_command = ["kubectl"] + args
    try:
        result = subprocess.run(full_command, capture_output=capture_output, text=True, check=True)
        return result
    except subprocess.CalledProcessError as e:
        print_error(f"kubectl command failed: {' '.join(full_command)}")
        print_error(f"Error: {e.stderr}")
        sys.exit(1)


def ask_for_confirmation() -> bool:
    """Ask user for confirmation to proceed"""
    response = (
        input(f"{Colors.BLUE.value}Do you approve these changes? (y/n){Colors.RESET.value}")
        .strip()
        .lower()
    )
    return response == "y"


def wait_until_y_or_n(question: str) -> bool:
    """Wait until user enters y or n. Cotinues asking until user enters y or n."""
    while True:
        response = input(f"{Colors.BLUE.value}{question} (y/n){Colors.RESET.value}").strip().lower()
        if response == "y" or response == "n":
            break
        print_error(f"Invalid response: {response}")
    return response == "y"
