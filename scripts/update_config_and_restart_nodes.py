#!/usr/bin/env python3

import argparse
from enum import Enum
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


class Colors(Enum):
    """ANSI color codes for terminal output"""

    RED = "\033[1;31m"
    YELLOW = "\033[1;33m"
    BLUE = "\033[1;34m"
    RESET = "\033[0m"


def print_colored(message: str, color: Colors = Colors.RESET, file=sys.stdout) -> None:
    """Print message with color"""
    print(f"{color.value}{message}{Colors.RESET.value}", file=file)


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

    parser.add_argument(
        "-o",
        "--config-overrides",
        action="append",
        help="Configuration overrides in key=value format. Can be specified multiple times. Example: --config-overrides consensus_manager_config.timeout=5000 --config-overrides validator_id=0x42",
    )

    return parser.parse_args()


def validate_arguments(args: argparse.Namespace) -> None:
    if args.num_nodes <= 0:
        print("Error: num-nodes must be a positive integer.", file=sys.stderr)
        sys.exit(1)


def parse_config_overrides(config_overrides: list[str]) -> dict[str, any]:
    """Parse config override strings in key=value format.

    Args:
        config_overrides: List of strings in "key=value" format

    Returns:
        dict: Dictionary mapping config keys to their values
    """
    if not config_overrides:
        return {}

    overrides = {}
    for override in config_overrides:
        if "=" not in override:
            print_colored(
                f"Error: Invalid config override format '{override}'. Expected 'key=value'",
                Colors.RED,
                file=sys.stderr,
            )
            sys.exit(1)

        # Split only on first '=' in case value contains '='
        key, value = override.split("=", 1)
        key = key.strip()
        value = value.strip()

        if not key:
            print(f"Error: Empty key in config override '{override}'", file=sys.stderr)
            sys.exit(1)

        # Try to convert value to appropriate type
        overrides[key] = parse_config_value(value)

    if not overrides:
        print("Error: No valid config overrides found", file=sys.stderr)
        sys.exit(1)

    return overrides


def parse_config_value(value: str) -> any:
    """Parse a string value and convert to appropriate type (int, float, bool, or string).

    Args:
        value: String value to parse

    Returns:
        The parsed value in appropriate type
    """
    # Try boolean first
    if value.lower() in ("true", "false"):
        return value.lower() == "true"

    # Try integer
    try:
        return int(value)
    except ValueError:
        pass

    # Try float
    try:
        return float(value)
    except ValueError:
        pass

    # If nothing else worked, return as string
    return value


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
        print_colored(
            f"kubectl command failed: {' '.join(full_command)}",
            Colors.RED,
            file=sys.stderr,
        )
        print_colored(f"Error: {e.stderr}", file=sys.stderr)
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
    if "config" not in config.get("data", {}):
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


def serialize_config_to_yaml(full_config: dict, config_data: dict) -> str:
    """Serialize configuration data back to YAML format.

    Args:
        full_config: The full YAML configuration dictionary
        config_data: The JSON configuration data to serialize and inject into the YAML config.

    Returns:
        str: The serialized YAML content
    """

    def represent_literal_str(dumper, data):
        """Custom representer for making sure that multi-line strings are represented as literal strings (using |)"""
        if "\n" in data:
            return dumper.represent_scalar("tag:yaml.org,2002:str", data, style="|")
        return dumper.represent_scalar("tag:yaml.org,2002:str", data)

    # Put the updated config back into the YAML
    full_config["data"]["config"] = json.dumps(config_data, indent=2)

    # Configure YAML dumper to use literal style for multi-line strings (see represent_literal_str).
    yaml.add_representer(str, represent_literal_str)

    # Convert back to YAML
    try:
        result = yaml.dump(full_config, default_flow_style=False, allow_unicode=True)
    finally:
        # Clean up the custom representer to avoid affecting other YAML operations
        yaml.add_representer(str, yaml.representer.SafeRepresenter.represent_str)

    return result


def update_config_values(
    config_content: str,
    node_id: int,
    config_overrides: dict[str, any] = None,
) -> str:
    """Update configuration values in the YAML content and return the updated YAML"""
    # Parse the configuration
    config, config_data = parse_config_from_yaml(config_content)

    for key, value in config_overrides.items():
        print(f"  Overriding config: {key} = {value}")
        config_data[key] = value

    # Serialize back to YAML
    return serialize_config_to_yaml(config, config_data)


def main():
    args = parse_arguments()
    validate_arguments(args)

    config_overrides = parse_config_overrides(args.config_overrides)
    if config_overrides:
        print(f"\nConfig overrides to apply:")
        for key, value in config_overrides.items():
            print(f"  {key} = {value}")
    else:
        print("No config overrides provided", file=sys.stderr)
        sys.exit(1)

    if not args.cluster:
        print_colored(
            "CLUSTER_PREFIX not provided. Assuming all nodes are on the current cluster",
            Colors.RED.value,
        )

    # Store original and updated configs for all nodes
    configs = {}

    # Process each node's configuration
    for node_id in range(args.num_nodes):
        print(f"\nProcessing node {node_id}...")

        # Get current config and normalize it (e.g. " vs ') to ensure not showing bogus diffs.
        original_config = get_configmap(args.namespace, node_id, args.cluster)

        # Update config
        updated_config = update_config_values(
            original_config, node_id, config_overrides
        )

        # Store configs
        configs[node_id] = {"original": original_config, "updated": updated_config}


if __name__ == "__main__":
    main()
