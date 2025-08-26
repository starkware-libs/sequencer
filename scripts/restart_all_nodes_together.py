#!/usr/bin/env python3
"""
Python equivalent of restart_all_nodes_together.sh
Restarts Kubernetes Apollo sequencer nodes with updated configuration
"""

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
        description="Restart Apollo sequencer nodes with updated configuration",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  %(prog)s -f feeder.integration-sepolia.starknet.io -n apollo-sepolia-integration -N 3
  %(prog)s --feeder-url feeder.integration-sepolia.starknet.io --namespace apollo-sepolia-integration --num-nodes 3 --cluster my-cluster
        """,
    )

    parser.add_argument(
        "-f",
        "--feeder-url",
        required=True,
        help="The feeder gateway URL (e.g., feeder.integration-sepolia.starknet.io)",
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
        print("Error: NUM_NODES must be a positive integer.", file=sys.stderr)
        sys.exit(1)


def get_current_block_number(feeder_url: str) -> int:
    """Get current block number from feeder gateway."""
    try:
        response = request.urlopen(f"https://{feeder_url}/feeder_gateway/get_block")
        block_data = json.loads(response.read().decode())
        return int(block_data["block_number"])
    except error.HTTPError as e:
        print(f"Error: Failed to get block number from feeder: {e}", file=sys.stderr)
        sys.exit(1)
    except (KeyError, ValueError) as e:
        print(f"Error: Invalid response format from feeder: {e}", file=sys.stderr)
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


def serialize_config_to_yaml(full_config: dict, config_data: dict) -> str:
    """Serialize configuration data back to YAML format.

    Args:
        full_config: The full YAML configuration dictionary
        config_data: The JSON configuration data to serialize

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

    # Configure YAML dumper to use literal style for multi-line strings
    yaml.add_representer(str, represent_literal_str)

    # Convert back to YAML
    try:
        result = yaml.dump(full_config, default_flow_style=False, allow_unicode=True)
    finally:
        # Clean up the custom representer to avoid affecting other YAML operations
        yaml.add_representer(str, yaml.representer.SafeRepresenter.represent_str)

    return result


def update_config_values(
    config_content: str, next_block_number: int, node_id: int
) -> str:
    """Update configuration values in the YAML content and return the updated YAML"""
    # Parse the configuration
    config, config_data = parse_config_from_yaml(config_content)

    # Update the consensus manager configuration values
    config_data["consensus_manager_config.immediate_active_height"] = next_block_number
    config_data["consensus_manager_config.cende_config.skip_write_height"] = (
        next_block_number
    )

    # Update validator_id
    validator_id = f"0x{node_id + 64:x}"
    config_data["validator_id"] = validator_id

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
        kubectl_args = ["apply", "-f", temp_file, "-n", f"{namespace}-{node_id}"]

        if cluster_prefix:
            kubectl_args.extend(["--context", f"{cluster_prefix}-{node_id}"])

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

    if not args.cluster:
        print_colored(
            "CLUSTER_PREFIX not provided. Assuming all nodes are on the current cluster",
            Colors.RED,
        )

    current_block_number = get_current_block_number(args.feeder_url)
    next_block_number = current_block_number + 1

    print(f"Current block number: {current_block_number}")
    print(f"Next block number: {next_block_number}")

    # Store original and updated configs for all nodes
    configs = {}

    # Process each node's configuration
    for node_id in range(args.num_nodes):
        print(f"\nProcessing node {node_id}...")

        # Get current config and normalize it (e.g. " vs ') to ensure not showing bogus diffs.
        original_config = normalize_config(
            get_configmap(args.namespace, node_id, args.cluster)
        )

        # Update config
        updated_config = update_config_values(
            original_config, next_block_number, node_id
        )

        # Store configs
        configs[node_id] = {"original": original_config, "updated": updated_config}

        # Show diff
        show_config_diff(original_config, updated_config, node_id)
        # TODO: Remove the break.
        break

    # Ask for confirmation
    if not ask_for_confirmation():
        print("Operation cancelled by user")
        sys.exit(1)

    # # Apply all configurations
    # print("\nApplying configurations...")
    # for node_id in range(args.num_nodes):
    #     print(f"Applying config for node {node_id}...")
    #     apply_configmap(
    #         configs[node_id]["updated"], args.namespace, node_id, args.cluster
    #     )

    # # Restart all pods
    # print("\nRestarting pods...")
    # for node_id in range(args.num_nodes):
    #     print(f"Restarting pod for node {node_id}...")
    #     restart_pod(args.namespace, node_id, args.cluster)

    print("\nAll nodes have been successfully restarted!")


if __name__ == "__main__":
    main()
