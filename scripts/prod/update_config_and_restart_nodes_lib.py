#!/usr/bin/env python3

import argparse
import json
import subprocess
import sys
from enum import Enum
from typing import Any, Optional

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


class ApolloArgsParserBuilder:
    """Builder class for creating argument parsers with required flags and custom arguments."""

    def __init__(self, description: str, usage_example: str):
        """Initialize the builder with usage example for epilog.

        Args:
            usage_example: String containing usage examples to be used as epilog
        """
        self.usage_example = usage_example
        self.parser = argparse.ArgumentParser(
            description=description,
            formatter_class=argparse.RawDescriptionHelpFormatter,
            epilog=usage_example,
        )

        self._add_common_flags()

    def _add_common_flags(self):
        """Add all common flags."""
        namespace_group = self.parser.add_mutually_exclusive_group(required=True)
        namespace_group.add_argument(
            "-n",
            "--namespace-prefix",
            help="The Kubernetes namespace prefix (e.g., apollo-sepolia-integration)",
        )
        namespace_group.add_argument(
            "-N",
            "--namespace-list",
            nargs="+",
            help="Space separated list of namespaces e.g., '--namespace-list apollo-sepolia-integration-0 apollo-sepolia-integration-2'",
        )

        self.parser.add_argument(
            "-m",
            "--num-nodes",
            type=int,
            help="The number of nodes to restart (required when specifying namespace-prefix)",
        )

        self.parser.add_argument(
            "-s",
            "--start-index",
            type=int,
            default=0,
            help="The starting index for node IDs (default: 0)",
        )

        cluster_group = self.parser.add_mutually_exclusive_group()
        cluster_group.add_argument(
            "-c", "--cluster-prefix", help="Optional cluster prefix for kubectl context"
        )
        cluster_group.add_argument(
            "-C",
            "--cluster-list",
            nargs="+",
            help="Space separated list of cluster names for kubectl contexts",
        )

        restart_group = self.parser.add_mutually_exclusive_group()
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

    def add_argument(self, *args, **kwargs):
        """Add a new argument to the parser.

        Args:
            *args: Positional arguments passed to parser.add_argument
            **kwargs: Keyword arguments passed to parser.add_argument
        """
        self.parser.add_argument(*args, **kwargs)
        return self

    def build(self) -> argparse.Namespace:
        """Build the argument parser, parse arguments, validate them, and return the result.

        Returns:
            argparse.Namespace: The parsed and validated arguments
        """
        args = self.parser.parse_args()
        validate_arguments(args)
        return args


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


def validate_arguments(args: argparse.Namespace) -> None:
    if (args.namespace_list and args.cluster_prefix) or (
        args.namespace_prefix and args.cluster_list
    ):
        print_error("Error: Use either list mode or prefix mode. You cannot mix them.")
        sys.exit(1)

    if args.namespace_list:
        # List mode.
        if args.start_index != 0:
            print_error("Error: start-index cannot be set when namespace-list is specified.")
            sys.exit(1)
        if args.num_nodes:
            print_error("Error: num-nodes cannot be set when namespace-list is specified.")
            sys.exit(1)
        if args.cluster_list:
            if len(args.cluster_list) != len(args.namespace_list):
                print_error(
                    "Error: cluster-list and namespace-list must have the same number of values."
                )
                sys.exit(1)
    else:
        # Prefix mode.
        if args.num_nodes is None:
            print_error("Error: num-nodes is required when not in namespace-list mode.")
            sys.exit(1)

        if args.num_nodes <= 0:
            print_error("Error: num-nodes must be a positive integer.")
            sys.exit(1)

        if args.start_index < 0:
            print_error("Error: start-index must be a non-negative integer.")
            sys.exit(1)


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


def run_kubectl_command(args: list, capture_output: bool = True) -> subprocess.CompletedProcess:
    full_command = ["kubectl"] + args
    try:
        result = subprocess.run(full_command, capture_output=capture_output, text=True, check=True)
        return result
    except subprocess.CalledProcessError as e:
        print_error(f"kubectl command failed: {' '.join(full_command)}")
        print_error(f"Error: {e.stderr}")
        sys.exit(1)


def get_namespace_args(namespace: str, cluster: Optional[str] = None) -> list[str]:
    ret = ["-n", f"{namespace}"]
    if cluster:
        ret.extend(["--context", f"{cluster}"])
    return ret


def get_configmap(
    namespace: str,
    cluster: Optional[str] = None,
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
    kubectl_args.extend(get_namespace_args(namespace, cluster))

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
    config_overrides: dict[str, Any] = None,
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


def show_config_diff(old_content: str, new_content: str, index: int) -> None:
    print_colored(
        f"--------------------- Config changes {index} --------------------",
        Colors.YELLOW,
    )

    old_lines = old_content.splitlines(keepends=True)
    new_lines = new_content.splitlines(keepends=True)

    diff = unified_diff(
        old_lines,
        new_lines,
        fromfile=f"config{index}.yaml_old",
        tofile=f"config{index}.yaml",
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
    index: int,
    cluster: Optional[str] = None,
) -> None:
    """Apply updated configmap"""
    with tempfile.NamedTemporaryFile(mode="w", suffix=".yaml", delete=False) as f:
        f.write(config_content)
        temp_file = f.name

        try:
            kubectl_args = ["apply", "-f", temp_file]
            kubectl_args.extend(get_namespace_args(namespace, cluster))

            run_kubectl_command(kubectl_args, capture_output=False)

        except Exception as e:
            print_error(f"Failed applying config for index {index}: {e}")
            sys.exit(1)


def get_pod_names(namespace: str, service: Service, cluster: Optional[str] = None) -> list[str]:
    """Get the list of pods for a specific service"""
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

    return pods


def restart_pods(
    namespace: str, pods: list[str], index: int, cluster: Optional[str] = None
) -> None:
    """Restart pod by deleting it"""

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
        kubectl_args.extend(get_namespace_args(namespace, cluster))

        try:
            run_kubectl_command(kubectl_args, capture_output=False)
            print_colored(f"Restarted {pod} for node {index}")
        except Exception as e:
            print_error(f"Failed restarting {pod} for node {index}: {e}")
            sys.exit(1)


def restart_node(
    namespace: str, service: Service, index: int, cluster: Optional[str] = None
) -> None:
    """Restart a single node by deleting its pod"""
    pods = get_pod_names(namespace, service, cluster)
    restart_pods(namespace, pods, index, cluster)


def restart_all_nodes(
    namespace_list: list[str],
    service: Service,
    cluster_list: Optional[list[str]] = None,
) -> None:
    """Restart nodes by deleting their pods"""
    for index, namespace in enumerate(namespace_list):
        cluster = cluster_list[index] if cluster_list else None
        restart_node(namespace, service, index, cluster)
    print_colored("\nAll pods have been successfully restarted!", Colors.GREEN)


def update_config(
    config_overrides: dict[str, Any],
    namespace_list: list[str],
    service: Service,
    cluster_list: Optional[list[str]] = None,
    restart_nodes: bool = True,
) -> None:
    assert config_overrides is not None, "config_overrides must be provided"
    assert namespace_list is not None and len(namespace_list) > 0, "namespaces must be provided"

    if not cluster_list:
        print_colored(
            "cluster-prefix/cluster-list not provided. Assuming all nodes are on the current cluster",
            Colors.RED,
        )
    else:
        assert len(cluster_list) == len(
            namespace_list
        ), f"cluster_list must have the same number of values as namespace_list. cluster_list: {cluster_list}, namespace_list: {namespace_list}"

    # Store original and updated configs for all nodes
    configs = []

    # Process each node's configuration
    for index, namespace in enumerate(namespace_list):
        cluster = cluster_list[index] if cluster_list else None
        print_colored(
            f"\nProcessing node for namespace {namespace} (cluster: {cluster if cluster else 'current cluster'})..."
        )

        # Get current config and normalize it (e.g. " vs ') to ensure not showing bogus diffs.
        original_config = normalize_config(get_configmap(namespace, cluster, service))

        # Update config
        updated_config = update_config_values(original_config, config_overrides)

        # Store configs
        configs.append({"original": original_config, "updated": updated_config})

        # Show diff
        show_config_diff(original_config, updated_config, index)

    if not ask_for_confirmation():
        print_error("Operation cancelled by user")
        sys.exit(1)

    # Apply all configurations
    print_colored("\nApplying configurations...")
    for index, config in enumerate(configs):
        print(f"Applying config {index}...")
        apply_configmap(
            config["updated"],
            namespace_list[index],
            index,
            cluster_list[index] if cluster_list else None,
        )

    print("\nUpdate completed successfully!")


def update_config_and_restart_nodes(
    config_overrides: dict[str, Any],
    namespace_list: list[str],
    service: Service,
    cluster_list: Optional[list[str]] = None,
    restart_nodes: bool = True,
) -> None:
    update_config(config_overrides, namespace_list, service, cluster_list, restart_nodes)
    if restart_nodes:
        restart_all_nodes(namespace_list, service, cluster_list)
    else:
        print_colored("\nSkipping pod restart (--no-restart was specified)")
