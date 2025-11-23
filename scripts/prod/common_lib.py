#!/usr/bin/env python3

import argparse
import subprocess
import sys
from enum import Enum
from typing import Optional


class Colors(Enum):
    """ANSI color codes for terminal output"""

    RED = "\033[1;31m"
    GREEN = "\033[1;32m"
    YELLOW = "\033[1;33m"
    BLUE = "\033[1;34m"
    RESET = "\033[0m"


def print_colored(message: str, color: Colors = Colors.RESET, file=sys.stdout) -> None:
    """Print message with color"""
    print(f"{color.value}{message}{Colors.RESET.value}", file=file)


def print_error(message: str) -> None:
    print_colored(message, color=Colors.RED, file=sys.stderr)


class RestartStrategy(Enum):
    """Strategy for restarting nodes."""

    ALL_AT_ONCE = "all_at_once"
    ONE_BY_ONE = "one_by_one"
    NO_RESTART = "no_restart"


def restart_strategy_converter(strategy_name: str) -> RestartStrategy:
    """Convert string to RestartStrategy enum with informative error message"""
    RESTART_STRATEGY_PREFIX = f"{RestartStrategy.__name__}."
    if strategy_name.startswith(RESTART_STRATEGY_PREFIX):
        strategy_name = strategy_name[len(RESTART_STRATEGY_PREFIX) :]

    strategy_name = strategy_name.lower()

    try:
        return RestartStrategy(strategy_name)
    except KeyError:
        valid_strategies = ", ".join([strategy.value for strategy in RestartStrategy])
        raise argparse.ArgumentTypeError(
            f"Invalid restart strategy '{strategy_name}'. Valid options are: {valid_strategies}"
        )


class Service(Enum):
    """Service types mapping to their configmap and pod names."""

    Core = ("sequencer-core-config", "sequencer-core-statefulset-0")
    Gateway = ("sequencer-gateway-config", "sequencer-gateway-deployment")
    HttpServer = (
        "sequencer-httpserver-config",
        "sequencer-httpserver-deployment",
    )
    L1 = ("sequencer-l1-config", "sequencer-l1-deployment")
    Mempool = ("sequencer-mempool-config", "sequencer-mempool-deployment")
    SierraCompiler = (
        "sequencer-sierracompiler-config",
        "sequencer-sierracompiler-deployment",
    )

    def __init__(self, config_map_name: str, pod_name: str) -> None:
        self.config_map_name = config_map_name
        self.pod_name = pod_name


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
