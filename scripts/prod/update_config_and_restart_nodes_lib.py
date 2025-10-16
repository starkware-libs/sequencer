#!/usr/bin/env python3

from abc import ABC, abstractmethod
import argparse
import json
import subprocess
import sys
from enum import Enum
from typing import Any, Optional

import tempfile
import urllib.error
import urllib.parse
import urllib.request
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

        self.add_argument(
            "-t",
            "--restart-strategy",
            type=restart_strategy_converter,
            choices=list(RestartStrategy),
            default=RestartStrategy.ONE_BY_ONE,
            help="Strategy for restarting nodes (default: One_By_One)",
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
        updated_config_data = self._update_config_for_instance(config_data, instance_index)
        return serialize_config_to_yaml(config, updated_config_data)

    @abstractmethod
    def _update_config_for_instance(self, config_data: dict, instance_index: int) -> dict[str, any]:
        """Update configuration data for a specific instance.

        Args:
            config_data: Current configuration data dictionary
            instance_index: Index of the instance to update configuration for

        Returns:
            Updated configuration data dictionary
        """
        pass


class ConstConfigValuesUpdater(ConfigValuesUpdater):
    """Concrete implementation that applies constant configuration overrides."""

    def __init__(self, config_overrides: dict[str, any]):
        """Initialize with configuration overrides.

        Args:
            config_overrides: Dictionary of configuration keys and values to override
        """
        self.config_overrides = config_overrides

    def _update_config_for_instance(self, config_data: dict, instance_index: int) -> dict[str, any]:
        """Apply the same configuration overrides to the config data for each instance.

        Args:
            config_data: Current configuration data dictionary
            instance_index: Index of the instance (not used in this implementation)

        Returns:
            Updated configuration data dictionary with overrides applied
        """
        # Create a copy to avoid modifying the original
        updated_config = config_data.copy()

        # Apply all overrides
        for key, value in self.config_overrides.items():
            print_colored(f"  Overriding config: {key} = {value}")
            updated_config[key] = value

        return updated_config


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

    try:
        return RestartStrategy(strategy_name)
    except KeyError:
        valid_strategies = ", ".join([strategy.value for strategy in RestartStrategy])
        raise argparse.ArgumentTypeError(
            f"Invalid restart strategy '{strategy_name}'. Valid options are: {valid_strategies}"
        )


class ServiceRestarter(ABC):
    """Abstract class for restarting service instances."""

    def __init__(self, namespace_list: list[str], service: Service, cluster_list: list[str]):
        # TODO: Asserts.

        assert len(namespace_list) == len(
            cluster_list
        ), "namespace_list and cluster_list must have the same length"
        self.namespace_list = namespace_list
        self.service = service
        self.cluster_list = cluster_list

    @abstractmethod
    def restart_service(self, service: Service, instance_index: int) -> bool:
        """Restart service for a specific instance. If returns False, the restart process should be aborted."""
        pass

    # from_restart_strategy is a static method that returns the appropriate ServiceRestarter based on the restart strategy.
    @staticmethod
    def from_restart_strategy(
        restart_strategy: RestartStrategy,
        namespace_list: list[str],
        service: Service,
        cluster_list: list[str],
        post_restart_instructions: Optional[list[str]] = None,
    ) -> "ServiceRestarter":
        if restart_strategy == RestartStrategy.ONE_BY_ONE:
            return OneByOneServiceRestarter(
                namespace_list, service, cluster_list, post_restart_instructions
            )
        else:
            assert (
                not post_restart_instructions
            ), f"post_restart_instructions is not allowed for this {restart_strategy} strategy"

            if restart_strategy == RestartStrategy.ALL_AT_ONCE:
                return ImmediateServiceRestarter(namespace_list, service, cluster_list)
            elif restart_strategy == RestartStrategy.NO_RESTART:
                return NoOpServiceRestarter(namespace_list, service, cluster_list)
            else:
                raise ValueError(f"Invalid restart strategy: {restart_strategy}")


class OneByOneServiceRestarter(ServiceRestarter):
    """Restart service instances one by one. Show the instructions after each restart and wait for user confirmation."""

    def __init__(
        self,
        namespace_list: list[str],
        service: Service,
        cluster_list: list[str],
        post_restart_instructions: list[str],
    ):
        assert (
            post_restart_instructions
        ), "post_restart_instructions is required for one_by_one restart strategy"
        assert len(namespace_list) == len(
            post_restart_instructions
        ), "namespace_list and post_restart_instructions must have the same length"
        super().__init__(namespace_list, service, cluster_list)
        self.post_restart_instructions = post_restart_instructions

    def restart_service(self, service: Service, instance_index: int) -> bool:
        """Restart the instance one by one, waiting for user confirmation after each restart."""
        # restart_pod(
        #     self.namespace_list[instance_index],
        #     service,
        #     instance_index,
        #     self.cluster_list[instance_index],
        # )
        instructions = self.post_restart_instructions[instance_index]
        print_colored(f"Restarted pod.\n{instructions} ", Colors.YELLOW)
        # Don't ask in the case of the last instance.
        if instance_index != len(self.post_restart_instructions) - 1 and not wait_until_y_or_n(
            f"Do you want to restart the next pod?"
        ):
            return False

        return True


class NoOpServiceRestarter(ServiceRestarter):
    """No-op service restarter."""

    def restart_service(self, service: Service, instance_index: int) -> bool:
        """No-op."""
        print_colored("\nSkipping pod restart.")
        return True


class ImmediateServiceRestarter(ServiceRestarter):
    """Restart service instances immediately."""

    def restart_service(self, service: Service, instance_index: int) -> bool:
        """Restart the instance immediately without waiting."""
        # restart_pod(
        #     self.namespace_list[instance_index],
        #     service,
        #     instance_index,
        #     self.cluster_list[instance_index],
        # )
        return True


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

    # TODO: Move the info about cluster-prefix/cluster-list not being set to here.


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


def wait_until_y_or_n(question: str) -> bool:
    """Wait until user enters y or n. Cotinues asking until user enters y or n."""
    while True:
        response = input(f"{Colors.BLUE.value}{question} (y/n){Colors.RESET.value}").strip().lower()
        if response == "y" or response == "n":
            break
        print_error(f"Invalid response: {response}")
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
    config_updater: ConfigValuesUpdater,
    namespace_list: list[str],
    service: Service,
    cluster_list: Optional[list[str]],
    restart_strategy: RestartStrategy,
    # TODO(guy.f): Add more abstraction to restart strategy so that the instructions are abstracted as part of it.
    post_restart_instructions: Optional[list[str]] = None,
) -> None:
    assert config_updater is not None, "config_updater must be provided"
    assert namespace_list is not None and len(namespace_list) > 0, "namespaces must be provided"

    if post_restart_instructions is not None:
        assert len(post_restart_instructions) == len(
            namespace_list
        ), f"post_restart_instructions must have the same length as namespace_list. logs_explorer_urls: {len(post_restart_instructions)}, namespace_list: {len(namespace_list)}"

    if not cluster_list:
        print_colored(
            "cluster-prefix/cluster-list not provided. Assuming all nodes are on the current cluster",
            Colors.RED,
        )
    else:
        assert len(cluster_list) == len(
            namespace_list
        ), f"cluster_list must have the same number of values as namespace_list. cluster_list: {cluster_list}, namespace_list: {namespace_list}"

    service_restarter = ServiceRestarter.from_restart_strategy(
        restart_strategy, namespace_list, service, cluster_list, post_restart_instructions
    )

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
        updated_config = config_updater.get_updated_config(original_config, index)

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
        # apply_configmap(
        #     config["updated"],
        #     namespace_list[index],
        #     index,
        #     cluster_list[index] if cluster_list else None,
        # )

    for index, config in enumerate(configs):
        if not service_restarter.restart_service(service, index):
            print_colored("\nAborting restart process.")
            return
        print_colored("\nAll pods have been successfully restarted!", Colors.GREEN)

    print("\nOperation completed successfully!")
