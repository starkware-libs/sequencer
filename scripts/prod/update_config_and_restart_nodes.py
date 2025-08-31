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


def print_error(message: str) -> None:
    print_colored(message, color=Colors.RED, file=sys.stderr)


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Update configuration for Apollo sequencer nodes and (optionally) restart them",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  %(prog)s --namespace apollo-sepolia-integration --num-nodes 3 --cluster my-cluster --config-overrides consensus_manager_config.timeout=5000 --config-overrides validator_id=0x42
  %(prog)s -n apollo-sepolia-integration -N 3 --config-overrides consensus_manager_config.timeout=5000 --config-overrides validator_id=0x42
  %(prog)s -n apollo-sepolia-integration -N 3 --config-overrides validator_id=0x42 --no-restart
  %(prog)s -n apollo-sepolia-integration -N 3 --config-overrides validator_id=0x42 -r
        """,
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
        help="Configuration overrides in key=value format. Can be specified multiple times. "
        "Example: --config-overrides consensus_manager_config.timeout=5000 "
        '--config-overrides components.gateway.url=\\"localhost\\" (note the escaping of the ")',
    )

    restart_group = parser.add_mutually_exclusive_group()
    restart_group.add_argument(
        "-r",
        "--restart-nodes",
        action="store_true",
        default=None,
        help="Restart the pods after updating configuration (default behavior)",
    )
    restart_group.add_argument(
        "--no-restart",
        action="store_true",
        help="Do not restart the pods after updating configuration",
    )

    return parser.parse_args()


def validate_arguments(args: argparse.Namespace) -> None:
    if args.num_nodes <= 0:
        print_error("Error: num-nodes must be a positive integer.")
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
        try:
            overrides[key] = json.loads(value)
        except (json.JSONDecodeError, TypeError) as e:
            print_error(
                f"Error: Invalid value '{value}' for key '{key}': {e}\n"
                'Did you remember to wrap string values in \\" ?'
            )
            sys.exit(1)

    if not overrides:
        print("Error: No valid config overrides found", file=sys.stderr)
        sys.exit(1)

    return overrides


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
        print_error(f"kubectl command failed: {' '.join(full_command)}")
        print_error(f"Error: {e.stderr}")
        sys.exit(1)


def get_namespace_params(
    namespace: str, node_id: int, cluster_prefix: Optional[str] = None
) -> list[str]:
    ret = ["-n", f"{namespace}-{node_id}"]
    if cluster_prefix:
        ret.extend(["--context", f"{cluster_prefix}-{node_id}"])
    return ret


def get_configmap(
    namespace: str, node_id: int, cluster_prefix: Optional[str] = None
) -> str:
    """Get configmap YAML for a specific node"""
    # TODO(guy.f): See if we can get the output as JSON and apply as JSON without going through YAML.
    kubectl_args = [
        "get",
        "cm",
        "sequencer-core-config",
        "-o",
        "yaml",
    ]
    kubectl_args.extend(get_namespace_params(namespace, node_id, cluster_prefix))

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
        print_error(f"Error parsing YAML: {e}")
        sys.exit(1)

    # The actual configuration is stored in the data section as a flattened config string
    if "config" not in config.get("data", {}):
        print_error("Error: Expected config not found in configmap")
        sys.exit(1)

    # Parse the flattened config format
    try:
        config_str = config["data"]["config"].strip()
        # Parse the JSON-like config string
        config_data = json.loads(config_str)
    except json.JSONDecodeError as e:
        print_error(f"Error parsing config JSON: {e}")
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


def normalize_config(config_content: str) -> str:
    """Normalize configuration by parsing and re-serializing without changes.

    This ensures consistent formatting for accurate diff comparison.
    """
    config, config_data = parse_config_from_yaml(config_content)
    return serialize_config_to_yaml(config, config_data)


def show_config_diff(old_content: str, new_content: str, node_id: int) -> None:
    print_colored(
        f"--------------------- Config changes to node no. {node_id}'s core service --------------------",
        Colors.YELLOW,
    )

    old_lines = old_content.splitlines(keepends=True)
    new_lines = new_content.splitlines(keepends=True)

    diff = unified_diff(
        old_lines,
        new_lines,
        fromfile=f"config{node_id}.yaml_old",
        tofile=f"config{node_id}.yaml",
        lineterm="",
    )

    diff_output = "".join(diff)
    if diff_output:
        print(diff_output)
    else:
        print("No changes detected")


def ask_for_confirmation() -> bool:
    """Ask user for confirmation to proceed"""
    while True:
        response = (
            input(f"{Colors.BLUE}Do you approve these changes? (y/n){Colors.RESET}")
            .strip()
            .lower()
        )
        if response == "y":
            return True
        elif response == "n":
            return False
        else:
            print("Please enter 'y' for yes or 'n' for no.")


def apply_configmap(
    config_content: str,
    namespace: str,
    node_id: int,
    cluster_prefix: Optional[str] = None,
) -> None:
    """Apply updated configmap"""
    with tempfile.NamedTemporaryFile(mode="w", suffix=".yaml", delete=False) as f:
        f.write(config_content)
        temp_file = f.name

    try:
        kubectl_args = ["apply", "-f", temp_file].extend(
            get_namespace_params(namespace, node_id, cluster_prefix)
        )

        run_kubectl_command(kubectl_args, capture_output=False)

    except Exception as e:
        print(f"Failed applying config for node {node_id}: {e}", file=sys.stderr)
        sys.exit(1)
    finally:
        os.unlink(temp_file)


def restart_pod(
    namespace: str, node_id: int, cluster_prefix: Optional[str] = None
) -> None:
    """Restart pod by deleting it"""
    kubectl_args = [
        "delete",
        "pod",
        "sequencer-core-statefulset-0",
        "-n",
        f"{namespace}-{node_id}",
    ]

    if cluster_prefix:
        kubectl_args.extend(["--context", f"{cluster_prefix}-{node_id}"])

    try:
        run_kubectl_command(kubectl_args, capture_output=False)
    except Exception as e:
        print(f"Failed restarting core pod for node {node_id}: {e}", file=sys.stderr)
        sys.exit(1)


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
        print_colored(f"\nProcessing node {node_id}...")

        # Get current config and normalize it (e.g. " vs ') to ensure not showing bogus diffs.
        original_config = normalize_config(
            get_configmap(args.namespace, node_id, args.cluster)
        )

        # Update config
        updated_config = update_config_values(
            original_config, node_id, config_overrides
        )

        # Store configs
        configs[node_id] = {"original": original_config, "updated": updated_config}

        # Show diff
        show_config_diff(original_config, updated_config, node_id)

    if not ask_for_confirmation():
        print("Operation cancelled by user")
        sys.exit(1)

    # Apply all configurations
    print("\nApplying configurations...")
    for node_id in range(args.num_nodes):
        print(f"Applying config for node {node_id}...")
        apply_configmap(
            configs[node_id]["updated"], args.namespace, node_id, args.cluster
        )

    # Restart is the default so only "don't restart" if explicitly specified
    should_restart = not args.no_restart

    # Restart all pods only if restart should happen
    if should_restart:
        print("\nRestarting pods...")
        for node_id in range(args.num_nodes):
            print(f"Restarting pod for node {node_id}...")
            restart_pod(args.namespace, node_id, args.cluster)
        print("\nAll nodes have been successfully restarted!")
    else:
        print("\nSkipping pod restart (--no-restart was specified)")

    print("\nOperation completed successfully!")


if __name__ == "__main__":
    main()
