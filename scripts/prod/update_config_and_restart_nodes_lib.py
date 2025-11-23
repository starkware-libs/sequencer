#!/usr/bin/env python3

import argparse
import json
import sys
<<<<<<< HEAD
import tempfile
import urllib.error
import urllib.parse
import urllib.request
from abc import ABC, abstractmethod
from difflib import unified_diff
||||||| 912efc99a
from enum import Enum
=======
from abc import ABC, abstractmethod
>>>>>>> origin/main-v0.14.1
from typing import Any, Optional

<<<<<<< HEAD
||||||| 912efc99a
import tempfile
import urllib.parse
=======
import tempfile
import urllib.error
import urllib.request
>>>>>>> origin/main-v0.14.1
import yaml
<<<<<<< HEAD
from common_lib import (
    Colors,
    NamespaceAndInstructionArgs,
    RestartStrategy,
    Service,
    ask_for_confirmation,
    get_namespace_args,
    print_colored,
    print_error,
    restart_strategy_converter,
    run_kubectl_command,
)
from restarter_lib import ServiceRestarter
||||||| 912efc99a
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
=======
from common_lib import (
    Colors,
    NamespaceAndInstructionArgs,
    RestartStrategy,
    Service,
    ask_for_confirmation,
    get_namespace_args,
    print_colored,
    print_error,
    restart_strategy_converter,
    run_kubectl_command,
)
from difflib import unified_diff
from restarter_lib import ServiceRestarter
>>>>>>> origin/main-v0.14.1


class ApolloArgsParserBuilder:
    """Builder class for creating argument parsers with required flags and custom arguments."""

    # TODO(guy.f): If we need to exclude more than just the restart flag, create a more generic mechanism.
    def __init__(self, description: str, usage_example: str, include_restart_strategy: bool = True):
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

        self._add_common_flags(include_restart_strategy)

    def _add_common_flags(self, include_restart_strategy: bool):
        """Add all common flags.

        Args:
            include_restart_strategy: Whether to include the restart strategy flag.
        """
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

        if include_restart_strategy:
            self.add_argument(
                "-t",
                "--restart-strategy",
                type=restart_strategy_converter,
                choices=list(RestartStrategy),
                required=True,
                help="Strategy for restarting nodes",
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


def get_logs_explorer_url(
    query: str,
    project_name: Optional[str] = None,
) -> str:
    # We need to double escape '(' and ')', so first we replace only them with their escaped versions.
    query = query.replace("(", urllib.parse.quote("(")).replace(")", urllib.parse.quote(")"))

    # Now "normal" escape everything else
    query = urllib.parse.quote(query)

    escaped_project_name = urllib.parse.quote(project_name)
    return (
        f"https://console.cloud.google.com/logs/query;query={query}"
        f"?project={escaped_project_name}"
    )


class ConfigValuesUpdater(ABC):
    """Abstract class for updating configuration values for different service instances."""

    def get_updated_config(self, orig_config_yaml: str, instance_index: int) -> str:
        """Get updated configuration YAML for a specific instance.

        Args:
            orig_config_yaml: Original configuration as YAML string
            instance_index: Index of the instance to update configuration for

        Returns:
            Updated configuration as YAML string
        """
        config, config_data = parse_config_from_yaml(orig_config_yaml)
        updated_config_data = self.get_updated_config_for_instance(config_data, instance_index)
        return serialize_config_to_yaml(config, updated_config_data)

    @abstractmethod
    def get_updated_config_for_instance(
        self, config_data: dict[str, Any], instance_index: int
    ) -> dict[str, Any]:
        """Get updated configuration data for a specific instance.

        Args:
            config_data: Current configuration data dictionary
            instance_index: Index of the instance to update configuration for

        Returns:
            Updated configuration data dictionary
        """


class ConstConfigValuesUpdater(ConfigValuesUpdater):
    """Concrete implementation that applies constant configuration overrides."""

    def __init__(self, config_overrides: dict[str, Any]):
        """Initialize with configuration overrides.

        Args:
            config_overrides: Dictionary of configuration keys and values to override
        """
        self.config_overrides = config_overrides

    def get_updated_config_for_instance(
        self, config_data: dict[str, Any], instance_index: int
    ) -> dict[str, Any]:
        """Apply the same configuration overrides to the config data for each instance."""
        updated_config = config_data.copy()

        for key, value in self.config_overrides.items():
            print_colored(f"  Overriding config: {key} = {value}")
            updated_config[key] = value

        return updated_config


def get_current_block_number(feeder_url: str) -> int:
    """Get the current block number from the feeder URL."""
    try:
        url = f"https://{feeder_url}/feeder_gateway/get_block"
        with urllib.request.urlopen(url) as response:
            if response.status != 200:
                raise urllib.error.HTTPError(
                    url, response.status, "HTTP Error", response.headers, None
                )
            data = json.loads(response.read().decode("utf-8"))
            current_block_number = data["block_number"]
            return current_block_number

    except urllib.error.URLError as e:
        print_error(f"Failed to fetch block number from feeder URL: {e}")
        sys.exit(1)
    except KeyError as e:
        print_error(f"Unexpected response format from feeder URL: {e}")
        sys.exit(1)
    except json.JSONDecodeError as e:
        print_error(f"Failed to parse JSON response from feeder URL: {e}")
        sys.exit(1)


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
        n=0,  # context lines; no real 'context' in config files
    )

    diff_output = "".join(diff)
    if diff_output:
        print_colored(diff_output)
    else:
        print_colored("No changes detected", Colors.BLUE)


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


<<<<<<< HEAD
def update_config_and_restart_nodes(
    config_values_updater: ConfigValuesUpdater,
    namespace_and_instruction_args: NamespaceAndInstructionArgs,
||||||| 912efc99a
def restart_pod(
    namespace: str, service: Service, index: int, cluster: Optional[str] = None
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
        kubectl_args.extend(get_namespace_args(namespace, cluster))

        try:
            run_kubectl_command(kubectl_args, capture_output=False)
            print_colored(f"Restarted {pod} for node {index}")
        except Exception as e:
            print_error(f"Failed restarting {pod} for node {index}: {e}")
            sys.exit(1)


def update_config_and_restart_nodes(
    config_overrides: dict[str, Any],
    namespace_list: list[str],
=======
def _update_config(
    config_values_updater: ConfigValuesUpdater,
    namespace_and_instruction_args: NamespaceAndInstructionArgs,
>>>>>>> origin/main-v0.14.1
    service: Service,
<<<<<<< HEAD
    restarter: ServiceRestarter,
||||||| 912efc99a
    cluster_list: Optional[list[str]] = None,
    restart_nodes: bool = True,
    # TODO(guy.f): Remove this once we have metrics we use to decide based on.
    wait_between_restarts: bool = False,
    post_restart_instructions: Optional[list[str]] = None,
=======
>>>>>>> origin/main-v0.14.1
) -> None:
<<<<<<< HEAD
    assert config_values_updater is not None, "config_values_updater must be provided"
    assert namespace_and_instruction_args.namespace_list is not None, "namespaces must be provided"

    if not namespace_and_instruction_args.cluster_list:
        print_colored(
            "cluster-prefix/cluster-list not provided. Assuming all nodes are on the current cluster",
            Colors.RED,
        )

||||||| 912efc99a
    assert config_overrides is not None, "config_overrides must be provided"
    assert namespace_list is not None and len(namespace_list) > 0, "namespaces must be provided"

    if post_restart_instructions is not None:
        assert len(post_restart_instructions) == len(
            namespace_list
        ), f"logs_explorer_urls must have the same length as namespace_list. logs_explorer_urls: {len(post_restart_instructions)}, namespace_list: {len(namespace_list)}"

    if wait_between_restarts:
        assert (
            post_restart_instructions is not None
        ), "logs_explorer_urls must be provided when wait_between_restarts is True"
    else:
        assert (
            post_restart_instructions is None
        ), "logs_explorer_urls must be None when wait_between_restarts is False"

    if not cluster_list:
        print_colored(
            "cluster-prefix/cluster-list not provided. Assuming all nodes are on the current cluster",
            Colors.RED,
        )
    else:
        assert len(cluster_list) == len(
            namespace_list
        ), f"cluster_list must have the same number of values as namespace_list. cluster_list: {cluster_list}, namespace_list: {namespace_list}"

=======
    """Update and apply configurations for all nodes."""
>>>>>>> origin/main-v0.14.1
    # Store original and updated configs for all nodes
    configs = []

    # Process each node's configuration
    for index in range(namespace_and_instruction_args.size()):
        namespace = namespace_and_instruction_args.get_namespace(index)
        cluster = namespace_and_instruction_args.get_cluster(index)

        print_colored(
            f"\nProcessing node for namespace {namespace} (cluster: {cluster if cluster is not None else 'current cluster'})..."
        )

        # Get current config and normalize it (e.g. " vs ') to ensure not showing bogus diffs.
        original_config = normalize_config(
            get_configmap(
                namespace,
                cluster,
                service,
            )
        )

        # Update config
        updated_config = config_values_updater.get_updated_config(original_config, index)

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
            namespace_and_instruction_args.get_namespace(index),
            index,
            namespace_and_instruction_args.get_cluster(index),
        )

<<<<<<< HEAD
    for index, config in enumerate(configs):
        if not restarter.restart_service(index):
            print_colored("\nAborting restart process.")
            sys.exit(1)

    print_colored("\nAll pods have been successfully restarted!", Colors.GREEN)
||||||| 912efc99a
    if restart_nodes:
        for index, config in enumerate(configs):
            restart_pod(
                namespace_list[index], service, index, cluster_list[index] if cluster_list else None
            )
            if wait_between_restarts:
                instructions = post_restart_instructions[index]
                print_colored(f"Restarted pod.\n{instructions}. ", Colors.YELLOW)
                # Don't ask in the case of the last job.
                if index != len(configs) - 1 and not wait_until_y_or_n(
                    f"Do you want to restart the next pod?"
                ):
                    print_colored("\nAborting restart process.")
                    return
        print_colored("\nAll pods have been successfully restarted!", Colors.GREEN)
    else:
        print_colored("\nSkipping pod restart (--no-restart was specified)")
=======

def update_config_and_restart_nodes(
    config_values_updater: Optional[ConfigValuesUpdater],
    namespace_and_instruction_args: NamespaceAndInstructionArgs,
    service: Service,
    restarter: ServiceRestarter,
) -> None:
    assert namespace_and_instruction_args.namespace_list is not None, "namespaces must be provided"

    if not namespace_and_instruction_args.cluster_list:
        print_colored(
            "cluster-prefix/cluster-list not provided. Assuming all nodes are on the current cluster",
            Colors.RED,
        )

    if config_values_updater is not None:
        _update_config(config_values_updater, namespace_and_instruction_args, service)

    for index in range(namespace_and_instruction_args.size()):
        if not restarter.restart_service(index):
            print_colored("\nAborting restart process.")
            sys.exit(1)
>>>>>>> origin/main-v0.14.1

    print("\nOperation completed successfully!")
