#!/usr/bin/env python3

import argparse
import json
import subprocess
import sys
from enum import Enum
from typing import Optional

import tempfile
import yaml
from difflib import unified_diff


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


class Service(Enum):
    """Service types mapping to their configmap and pod names."""

    Core = ("sequencer-core-config", "sequencer-core-statefulset-0")
    SequencerGateway = ("sequencer-gateway-config", "sequencer-gateway-deployment")
    SequencerHttpserver = (
        "sequencer-httpserver-config",
        "sequencer-httpserver-deployment",
    )
    SequencerL1 = ("sequencer-l1-config", "sequencer-l1-deployment")
    SequencerMempool = ("sequencer-mempool-config", "sequencer-mempool-deployment")
    SequencerSierracompiler = (
        "sequencer-sierracompiler-config",
        "sequencer-sierracompiler-deployment",
    )

    def __init__(self, config_map_name: str, pod_name: str) -> None:
        self.config_map_name = config_map_name
        self.pod_name = pod_name


def service_type_converter(service_name: str) -> Service:
    """Convert string to Service enum with informative error message"""
    try:
        return Service[service_name]
    except KeyError:
        valid_services = ", ".join([service.name for service in Service])
        raise argparse.ArgumentTypeError(
            f"Invalid service type '{service_name}'. Valid options are: {valid_services}"
        )


def build_args_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Update configuration for the services and (optionally) restart them",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Update sequencer core configuration (default service)
  %(prog)s --namespace apollo-sepolia-integration --num-nodes 3 --cluster my-cluster --config-overrides consensus_manager_config.timeout=5000 --config-overrides validator_id=0x42
  %(prog)s -n apollo-sepolia-integration -N 3 --config-overrides consensus_manager_config.timeout=5000 --config-overrides validator_id=0x42
  
  # Update gateway configuration
  %(prog)s -n apollo-sepolia-integration -N 3 -j SequencerGateway --config-overrides gateway_config.port=8080
  
  # Update mempool configuration
  %(prog)s -n apollo-sepolia-integration -N 3 -j SequencerMempool --config-overrides mempool_config.max_size=1000
  
  # Update L1 provider configuration
  %(prog)s -n apollo-sepolia-integration -N 3 -j SequencerL1 --config-overrides l1_config.endpoint=https://eth-mainnet.alchemyapi.io/v2/your-key
  
  # Update without restart
  %(prog)s -n apollo-sepolia-integration -N 3 --config-overrides validator_id=0x42 --no-restart
  
  # Update with explicit restart
  %(prog)s -n apollo-sepolia-integration -N 3 --config-overrides validator_id=0x42 -r
  
  # Update starting from specific node index
  %(prog)s -n apollo-sepolia-integration -N 3 -s 5 --config-overrides validator_id=0x42
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
        "-s",
        "--start-index",
        type=int,
        default=0,
        help="The starting index for node IDs (default: 0)",
    )

    parser.add_argument("-c", "--cluster", help="Optional cluster prefix for kubectl context")

    parser.add_argument(
        "-j",
        "--service",
        type=service_type_converter,
        choices=list(Service),
        default=Service.Core,
        help="Service type to operate on; determines configmap and pod names (default: sequencer-core)",
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

    return parser


def validate_arguments(args: argparse.Namespace) -> None:
    if args.num_nodes <= 0:
        print_error("Error: num-nodes must be a positive integer.")
        sys.exit(1)

    if args.start_index < 0:
        print_error("Error: start-index must be a non-negative integer.")
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
            print_error(f"Error: Empty key in config override '{override}'")
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
        print_error("Error: No valid config overrides found")
        sys.exit(1)

    return overrides


def run_kubectl_command(args: list, capture_output: bool = True) -> subprocess.CompletedProcess:
    full_command = ["kubectl"] + args
    try:
        result = subprocess.run(full_command, capture_output=capture_output, text=True, check=True)
        return result
    except subprocess.CalledProcessError as e:
        print_error(f"kubectl command failed: {' '.join(full_command)}")
        print_error(f"Error: {e.stderr}")
        sys.exit(1)


def get_namespace_args(
    namespace: str, node_id: int, cluster_prefix: Optional[str] = None
) -> list[str]:
    ret = ["-n", f"{namespace}-{node_id}"]
    if cluster_prefix:
        # For node_id 0 just use the cluster prefix, otherwise use cluster_prefix-{node_id}.
        ret.extend(["--context", f"{cluster_prefix}-{node_id}" if node_id > 0 else cluster_prefix])
    return ret


def get_configmap(
    namespace: str,
    node_id: int,
    cluster_prefix: Optional[str] = None,
    service: Service = Service.Core,
) -> str:
    """Get configmap YAML for a specific node"""
    kubectl_args = [
        "get",
        "cm",
        service.config_map_name,
        "-o",
        "yaml",
    ]
    kubectl_args.extend(get_namespace_args(namespace, node_id, cluster_prefix))

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
        print_colored(f"  Overriding config: {key} = {value}")
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
        print_colored(diff_output)
    else:
        print_colored("No changes detected", Colors.BLUE)


def ask_for_confirmation() -> bool:
    """Ask user for confirmation to proceed"""
    response = (
        input(f"{Colors.BLUE.value}Do you approve these changes? (y/n){Colors.RESET.value}")
        .strip()
        .lower()
    )
    return response == "y"


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
            kubectl_args = ["apply", "-f", temp_file]
            kubectl_args.extend(get_namespace_args(namespace, node_id, cluster_prefix))

            run_kubectl_command(kubectl_args, capture_output=False)

        except Exception as e:
            print_error(f"Failed applying config for node {node_id}: {e}")
            sys.exit(1)


def restart_pod(
    namespace: str, node_id: int, service: Service, cluster_prefix: Optional[str] = None
) -> None:
    """Restart pod by deleting it"""
    # Get the list of pods (one string per line).
    kubectl_args = [
        "get",
        "pods",
        "-o",
        "name",
    ]
    pods = run_kubectl_command(kubectl_args, capture_output=True).stdout.splitlines()

    # Filter the list of pods to only include the ones that match the service and extract the pod name.
    pods = [pod.split("/")[1] for pod in pods if pod.startswith(f"pod/{service.pod_name}")]

    if not pods:
        print_error(f"Could not find pods for service {service.pod_name}.")
        sys.exit(1)

    # Go over each pod and delete it.
    for pod in pods:
        kubectl_args = [
            "delete",
            "pod",
            pod,
        ]
        kubectl_args.extend(get_namespace_args(namespace, node_id, cluster_prefix))

        try:
            run_kubectl_command(kubectl_args, capture_output=False)
            print_colored(f"Restarted {pod} for node {node_id}")
        except Exception as e:
            print_error(f"Failed restarting {pod} for node {node_id}: {e}")
            sys.exit(1)


def update_config_and_restart_nodes(
    config_overrides: dict[str, any],
    namespace: str,
    num_nodes: int,
    start_index: int,
    service: Service,
    cluster_prefix: Optional[str] = None,
    restart_nodes: bool = True,
) -> None:
    assert config_overrides is not None, "config_overrides must be provided"
    assert namespace is not None, "namespace must be provided"
    assert num_nodes > 0, "num_nodes must be a positive integer"
    assert start_index >= 0, "start_index must be a non-negative integer"

    if not cluster_prefix:
        print_colored(
            "CLUSTER_PREFIX not provided. Assuming all nodes are on the current cluster",
            Colors.RED.value,
        )

    # Store original and updated configs for all nodes
    configs = {}

    # Define the range of node IDs to process
    node_ids = range(start_index, start_index + num_nodes)

    # Process each node's configuration
    for node_id in node_ids:
        print_colored(f"\nProcessing node {node_id}...")

        # Get current config and normalize it (e.g. " vs ') to ensure not showing bogus diffs.
        original_config = normalize_config(
            get_configmap(namespace, node_id, cluster_prefix, service)
        )

        # Update config
        updated_config = update_config_values(original_config, node_id, config_overrides)

        # Store configs
        configs[node_id] = {"original": original_config, "updated": updated_config}

        # Show diff
        show_config_diff(original_config, updated_config, node_id)

    if not ask_for_confirmation():
        print_error("Operation cancelled by user")
        sys.exit(1)

    # Apply all configurations
    print_colored("\nApplying configurations...")
    for node_id in node_ids:
        print(f"Applying config for node {node_id}...")
        apply_configmap(configs[node_id]["updated"], namespace, node_id, cluster_prefix)

    if restart_nodes:
        for node_id in node_ids:
            restart_pod(namespace, node_id, service, cluster_prefix)
        print_colored("\nAll pods have been successfully restarted!", Colors.GREEN)
    else:
        print_colored("\nSkipping pod restart (--no-restart was specified)")

    print("\nOperation completed successfully!")
