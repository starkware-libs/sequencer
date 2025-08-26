#!/usr/bin/env python3

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
from difflib import unified_diff
from pathlib import Path
from typing import Optional

from urllib import request, error
import yaml


class Colors:
    """ANSI color codes for terminal output"""

    RED = "\033[1;31m"
    YELLOW = "\033[1;33m"
    BLUE = "\033[1;34m"
    RESET = "\033[0m"


def print_colored(message: str, color: str = Colors.RESET) -> None:
    """Print message with color"""
    print(f"{color}{message}{Colors.RESET}")


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Update configuration for Apollo sequencer nodes and (optionally) restart them",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )

    parser.add_argument(
        "-n",
        "--namespace",
        required=True,
        help="The Kubernetes namespace prefix (e.g., apollo-sepolia-integration)",
    )

    parser.add_argument(
        "-N",
        "--num-nodes",
        required=True,
        type=int,
        help="The number of nodes to restart (required)",
    )

    parser.add_argument(
        "-c", "--cluster", help="Optional cluster prefix for kubectl context"
    )

    return parser.parse_args()


def validate_arguments(args: argparse.Namespace) -> None:
    if args.num_nodes <= 0:
        print("Error: num-nodes must be a positive integer.", file=sys.stderr)
        sys.exit(1)


def run_kubectl_command(
    args: list, capture_output: bool = True
) -> subprocess.CompletedProcess:
    full_command = ["kubectl"] + args
    try:
        result = subprocess.run(
            full_command, capture_output=capture_output, text=True, check=True
        )
        return result
    except subprocess.CalledProcessError as e:
        print(f"kubectl command failed: {' '.join(full_command)}", file=sys.stderr)
        print(f"Error: {e.stderr}", file=sys.stderr)
        sys.exit(1)


def get_configmap(
    namespace: str, node_id: int, cluster_prefix: Optional[str] = None
) -> str:
    """Get configmap YAML for a specific node"""
    kubectl_args = [
        "get",
        "cm",
        "sequencer-core-config",
        "-n",
        f"{namespace}-{node_id}",
        "-o",
        "yaml",
    ]

    if cluster_prefix:
        kubectl_args.extend(["--context", f"{cluster_prefix}-{node_id}"])

    result = run_kubectl_command(kubectl_args)
    return result.stdout


def parse_config_from_yaml(config_content: str) -> tuple[dict, dict]:
    """Parse YAML config and extract the JSON configuration data.

    Returns:
        tuple: (full_config_dict, parsed_json_config_dict)
        parsed_json_config_dict: The internal config dictionary, as parsed from the config string.
    """
    # Parse YAML
    try:
        config = yaml.safe_load(config_content)
    except yaml.YAMLError as e:
        print(f"Error parsing YAML: {e}", file=sys.stderr)
        sys.exit(1)

    # The actual configuration is stored in the data section as a flattened config string
    if "data" not in config or "config" not in config["data"]:
        print("Error: Expected config not found in configmap", file=sys.stderr)
        sys.exit(1)

    # Parse the flattened config format
    try:
        config_str = config["data"]["config"].strip()
        # Parse the JSON-like config string
        config_data = json.loads(config_str)
    except json.JSONDecodeError as e:
        print(f"Error parsing config JSON: {e}", file=sys.stderr)
        sys.exit(1)

    return config, config_data


def main():
    args = parse_arguments()
    validate_arguments(args)

    if not args.cluster:
        print_colored(
            "CLUSTER_PREFIX not provided. Assuming all nodes are on the current cluster",
            Colors.RED,
        )

    # Process each node's configuration
    for node_id in range(args.num_nodes):
        print(f"\nProcessing node {node_id}...")

        # Get current config and normalize it (e.g. " vs ') to ensure not showing bogus diffs.
        original_config = get_configmap(args.namespace, node_id, args.cluster)


if __name__ == "__main__":
    main()
