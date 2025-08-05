#!/usr/bin/env python3
"""
Python script to increment nodes height in Kubernetes cluster.
Equivalent functionality to increment_nodes_height.sh
"""

import argparse
import json
import subprocess
import sys
import os
import re
import tempfile
import difflib
from typing import Optional
from urllib import request, error

# TODO(guy.f): I had to run `pip install PyYAML`. It appears in requirements.txt, how is it used?
import yaml


def setup_argument_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Increment nodes height in Kubernetes cluster",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  %(prog)s -f feeder.integration-sepolia.starknet.io -n apollo-sepolia-integration
  %(prog)s --feeder-url feeder.integration-sepolia.starknet.io --namespace apollo-sepolia-integration --cluster my-cluster
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
        "-c", "--cluster", help="Optional cluster prefix for kubectl context"
    )

    parser.add_argument(
        "-N",
        "--num-nodes",
        type=int,
        required=True,
        help="Number of nodes to process",
    )

    return parser


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
    command: list, capture_output: bool
) -> subprocess.CompletedProcess:
    """Runs kubectl command and handles errors."""
    try:
        result = subprocess.run(
            command, capture_output=capture_output, text=True, check=True
        )
        return result
    except subprocess.CalledProcessError as e:
        print(f"Error: kubectl command failed: {' '.join(command)}", file=sys.stderr)
        print(f"Error output: {e.stderr}", file=sys.stderr)
        sys.exit(1)


def get_configmap(
    namespace_prefix: str, node_id: int, cluster_prefix: Optional[str] = None
) -> str:
    """Gets configmap YAML content for a specific node."""
    command = [
        "kubectl",
        "get",
        "cm",
        "sequencer-core-config",
        "-n",
        f"{namespace_prefix}-{node_id}",
        "-o",
        "yaml",
    ]

    if cluster_prefix:
        command.extend(["--context", f"{cluster_prefix}-{node_id}"])

    result = run_kubectl_command(command, capture_output=True)
    return result.stdout


def update_config_content(
    config_content: str, next_block_number: int, node_id: int
) -> str:
    """Updates configuration content with new block number values."""
    # Parse YAML
    try:
        config_data = yaml.safe_load(config_content)
    except yaml.YAMLError as e:
        print(f"Error: Failed to parse YAML: {e}", file=sys.stderr)
        sys.exit(1)

    # Get the actual config from the data section
    config_yaml_str = config_data["data"]["config.yaml"]
    actual_config = yaml.safe_load(config_yaml_str)

    # Update the specified fields
    if "consensus_manager_config" in actual_config:
        actual_config["consensus_manager_config"][
            "immediate_active_height"
        ] = next_block_number

        if "cende_config" in actual_config["consensus_manager_config"]:
            actual_config["consensus_manager_config"]["cende_config"][
                "skip_write_height"
            ] = next_block_number

    # Update validator_id
    validator_id = f"0x{node_id + 64}"
    if "consensus_manager_config" in actual_config:
        actual_config["consensus_manager_config"]["validator_id"] = validator_id

    # Convert back to YAML string and update the configmap
    config_data["data"]["config.yaml"] = yaml.dump(
        actual_config, default_flow_style=False
    )

    return yaml.dump(config_data, default_flow_style=False)


def show_diff(old_content: str, new_content: str, node_id: int):
    """Show diff between old and new configuration."""
    print(
        f"\033[1;33m--------------------- Config changes to node no. {node_id}'s core service --------------------\033[0m"
    )

    old_lines = old_content.splitlines(keepends=True)
    new_lines = new_content.splitlines(keepends=True)

    diff = difflib.unified_diff(
        old_lines,
        new_lines,
        fromfile=f"config{node_id}.yaml_old",
        tofile=f"config{node_id}.yaml",
        lineterm="",
    )

    for line in diff:
        print(line.rstrip())


def apply_configmap(
    config_content: str,
    namespace_prefix: str,
    node_id: int,
    cluster_prefix: Optional[str] = None,
):
    """Apply updated configmap to Kubernetes."""
    # Write config to temporary file
    with tempfile.NamedTemporaryFile(mode="w", suffix=".yaml", delete=False) as f:
        f.write(config_content)
        temp_file = f.name

    try:
        command = [
            "kubectl",
            "apply",
            "-f",
            temp_file,
            "-n",
            f"{namespace_prefix}-{node_id}",
        ]

        if cluster_prefix:
            command.extend(["--context", f"{cluster_prefix}-{node_id}"])

        run_kubectl_command(command, capture_output=False)
        print(f"Successfully applied config for node {node_id}")

    finally:
        os.unlink(temp_file)


def restart_pod(
    namespace_prefix: str, node_id: int, cluster_prefix: Optional[str] = None
):
    """Restart the sequencer core pod."""
    command = [
        "kubectl",
        "delete",
        "pod",
        "sequencer-core-statefulset-0",
        "-n",
        f"{namespace_prefix}-{node_id}",
    ]

    if cluster_prefix:
        command.extend(["--context", f"{cluster_prefix}-{node_id}"])

    run_kubectl_command(command, capture_output=False)
    print(f"Successfully restarted core pod for node {node_id}")


def get_user_confirmation() -> bool:
    """Get user confirmation for applying changes."""
    while True:
        response = (
            input("\033[1;34mDo you approve these changes? (y/n)\033[0m")
            .strip()
            .lower()
        )
        if response == "y":
            return True
        elif response == "n":
            return False
        else:
            print("Please enter 'y' for yes or 'n' for no.")


def main():
    parser = setup_argument_parser()
    args = parser.parse_args()

    if not args.cluster:
        print(
            "\033[1;31mCLUSTER_PREFIX not provided. Assuming all nodes are on the current cluster\033[0m"
        )

    current_block_number = get_current_block_number(args.feeder_url)
    next_block_number = current_block_number + 1

    print(f"Current block number: {current_block_number}")
    print(f"Next block number: {next_block_number}")

    # Store configurations for all nodes
    configs = {}

    # Process each node (0, 1, 2)
    for node_id in range(args.num_nodes):
        print(f"\nProcessing node {node_id}...")

        # Get current config
        old_config = get_configmap(args.namespace, node_id, args.cluster)

        # Update config
        new_config = update_config_content(old_config, next_block_number, node_id)

        # Show diff
        show_diff(old_config, new_config, node_id)

        # Store for later application
        configs[node_id] = new_config

    # Get user confirmation
    if not get_user_confirmation():
        print("Changes not approved. Exiting.")
        sys.exit(1)

    # Apply configurations
    print("\nApplying configurations...")
    for node_id in range(args.num_nodes):
        try:
            apply_configmap(configs[node_id], args.namespace, node_id, args.cluster)
        except Exception as e:
            print(f"Failed applying config for node {node_id}: {e}", file=sys.stderr)
            sys.exit(1)

    # Restart pods
    print("\nRestarting pods...")
    for node_id in range(args.num_nodes):
        try:
            restart_pod(args.namespace, node_id, args.cluster)
        except Exception as e:
            print(
                f"Failed restarting core pod for node {node_id}: {e}", file=sys.stderr
            )
            sys.exit(1)

    print("\nAll operations completed successfully!")


if __name__ == "__main__":
    main()
