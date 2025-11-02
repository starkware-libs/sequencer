#!/usr/bin/env python3

import argparse
import json
import subprocess
import sys
from abc import ABC, abstractmethod
from enum import Enum
from time import sleep
from typing import Any, Callable, Optional

import signal
import socket
import tempfile
import urllib.error
import urllib.parse
import urllib.request
import yaml
from difflib import unified_diff
from prometheus_client.parser import text_string_to_metric_families


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

    strategy_name = strategy_name.lower()

    try:
        return RestartStrategy(strategy_name)
    except KeyError:
        valid_strategies = ", ".join([strategy.value for strategy in RestartStrategy])
        raise argparse.ArgumentTypeError(
            f"Invalid restart strategy '{strategy_name}'. Valid options are: {valid_strategies}"
        )


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


class MetricConditionGater:
    """Gates progress on a metric satisfying a condition.

    This class was meant to be used with counter/gauge metrics. It may not work properly with histogram metrics.
    """

    class MetricCondition:
        def __init__(
            self,
            value_condition: Callable[[Any], bool],
            condition_description: Optional[str] = None,
        ):
            self.value_condition = value_condition
            self.condition_description = condition_description

    def __init__(
        self,
        metric_name: str,
        namespace: str,
        cluster: Optional[str],
        pod: str,
        metrics_port: int,
        metric_value_condition: "MetricConditionGater.MetricCondition",
        refresh_interval_seconds: int = 3,
    ):
        self.metric_name = metric_name
        self.local_port = self._get_free_port()
        self.namespace = namespace
        self.cluster = cluster
        self.pod = pod
        self.metrics_port = metrics_port
        self.metric_value_condition = metric_value_condition
        self.refresh_interval_seconds = refresh_interval_seconds

    @staticmethod
    def _get_free_port():
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            s.bind(("", 0))
            return s.getsockname()[1]

    def _get_metrics_raw_string(self) -> str:
        while True:
            try:
                with urllib.request.urlopen(
                    f"http://localhost:{self.local_port}/monitoring/metrics"
                ) as response:
                    if response.status == 200:
                        return response.read().decode("utf-8")
                    else:
                        print_colored(
                            f"Failed to get metrics for pod {self.pod}: {response.status}"
                        )
            except urllib.error.URLError as e:
                print_colored(f"Failed to get metrics for pod {self.pod}: {e}")
            print_colored(
                f"Waiting {self.refresh_interval_seconds} seconds to retry getting metrics...",
                Colors.YELLOW,
            )
            sleep(self.refresh_interval_seconds)

    def _poll_until_condition_met(self):
        """Poll metrics until the condition is met for the metric."""
        condition_description = (
            f"({self.metric_value_condition.condition_description}) "
            if self.metric_value_condition.condition_description is not None
            else ""
        )

        while True:
            metrics = self._get_metrics_raw_string()
            assert metrics is not None, f"Failed to get metrics from for pod {self.pod}"

            metric_families = text_string_to_metric_families(metrics)
            val = None
            for metric_family in metric_families:
                if metric_family.name == self.metric_name:
                    if len(metric_family.samples) > 1:
                        print_error(
                            f"Multiple samples found for metric {self.metric_name}. Using the first one.",
                        )
                    val = metric_family.samples[0].value
                    break

            if val is None:
                print_colored(
                    f"Metric '{self.metric_name}' not found in pod {self.pod}. Assuming the node is not ready."
                )
            elif self.metric_value_condition.value_condition(val):
                print_colored(
                    f"Metric {self.metric_name} condition {condition_description}met (value={val})."
                )
                return
            else:
                print_colored(
                    f"Metric {self.metric_name} condition {condition_description}not met (value={val}). Continuing to wait."
                )

            sleep(self.refresh_interval_seconds)

    @staticmethod
    def _terminate_port_forward_process(pf_process: subprocess.Popen):
        if pf_process and pf_process.poll() is None:
            print_colored(f"Terminating kubectl port-forward process (PID: {pf_process.pid})")
            pf_process.terminate()
            try:
                pf_process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                print_colored("Force killing kubectl port-forward process")
                pf_process.kill()
                pf_process.wait()

    def gate(self):
        """Wait until the nodes metrics satisfy the condition."""
        # This method:
        # 1. Starts kubectl port forwarding to the node and keep it running in the background so we can access the metrics.
        # 2. Calls _poll_until_condition_met.
        # 3. Terminates the port forwarding process when done or when interrupted.
        cmd = [
            "kubectl",
            "port-forward",
            f"pod/{self.pod}",
            f"{self.local_port}:{self.metrics_port}",
        ]
        cmd.extend(get_namespace_args(self.namespace, self.cluster))

        pf_process = None

        try:
            pf_process = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            print("Waiting for forwarding to start")
            # Give the forwarding time to start.
            # TODO(guy.f): Consider poll until the forwarding is ready if we see any issues.
            sleep(3)
            assert (
                pf_process.poll() is None
            ), f"Port forwarding process exited with code {pf_process.returncode}"

            print(
                f"Forwarding started (from local port {self.local_port} to {self.pod}:{self.metrics_port})"
            )

            # Set up signal handler to ensure forwarding subprocess is terminated on interruption
            def signal_handler(signum, frame):
                self._terminate_port_forward_process(pf_process)
                sys.exit(0)

            signal.signal(signal.SIGINT, signal_handler)
            signal.signal(signal.SIGTERM, signal_handler)

            self._poll_until_condition_met()

        finally:
            self._terminate_port_forward_process(pf_process)


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


class ServiceRestarter(ABC):
    """Abstract class for restarting service instances."""

    def __init__(
        self,
        namespace_and_instruction_args: NamespaceAndInstructionArgs,
        service: Service,
    ):
        self.namespace_and_instruction_args = namespace_and_instruction_args
        self.service = service

    @staticmethod
    def _restart_pod(
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
        kubectl_args.extend(get_namespace_args(namespace, cluster))
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

    @abstractmethod
    def restart_service(self, instance_index: int) -> bool:
        """Restart service for a specific instance. If returns False, the restart process should be aborted."""

    # from_restart_strategy is a static method that returns the appropriate ServiceRestarter based on the restart strategy.
    @staticmethod
    def from_restart_strategy(
        restart_strategy: RestartStrategy,
        namespace_and_instruction_args: NamespaceAndInstructionArgs,
        service: Service,
    ) -> "ServiceRestarter":
        if restart_strategy == RestartStrategy.ONE_BY_ONE:
            check_between_restarts = lambda instance_index: (
                True
                if instance_index == namespace_and_instruction_args.size() - 1
                else wait_until_y_or_n(f"Do you want to restart the next pod?")
            )

            return ChecksBetweenRestarts(
                namespace_and_instruction_args,
                service,
                check_between_restarts,
            )
        elif restart_strategy == RestartStrategy.ALL_AT_ONCE:
            return ChecksBetweenRestarts(
                namespace_and_instruction_args,
                service,
                lambda instance_index: True,
            )
        elif restart_strategy == RestartStrategy.NO_RESTART:
            assert (
                namespace_and_instruction_args.get_instruction(0) is None
            ), f"post_restart_instructions is not allowed with no_restart as the restart strategy"
            return NoOpServiceRestarter(namespace_and_instruction_args, service)
        else:
            raise ValueError(f"Invalid restart strategy: {restart_strategy}")


class ChecksBetweenRestarts(ServiceRestarter):
    """Checks between restarts."""

    def __init__(
        self,
        namespace_and_instruction_args: NamespaceAndInstructionArgs,
        service: Service,
        check_between_restarts: Callable[[int], bool],
    ):
        super().__init__(namespace_and_instruction_args, service)
        self.check_between_restarts = check_between_restarts

    def restart_service(self, instance_index: int) -> bool:
        """Restart the instance one by one, running the use code in between each restart."""
        self._restart_pod(
            self.namespace_and_instruction_args.get_namespace(instance_index),
            self.service,
            instance_index,
            self.namespace_and_instruction_args.get_cluster(instance_index),
        )
        instructions = self.namespace_and_instruction_args.get_instruction(instance_index)
        print_colored(
            f"Restarted pod {instance_index}.\n{instructions if instructions is not None else ''} ",
            Colors.YELLOW,
        )
        return self.check_between_restarts(instance_index)


class NoOpServiceRestarter(ServiceRestarter):
    """No-op service restarter."""

    def restart_service(self, instance_index: int) -> bool:
        """No-op."""
        print_colored("\nSkipping pod restart.")
        return True


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


def update_config_and_restart_nodes(
    config_values_updater: ConfigValuesUpdater,
    namespace_and_instruction_args: NamespaceAndInstructionArgs,
    service: Service,
    restarter: ServiceRestarter,
) -> None:
    assert config_values_updater is not None, "config_values_updater must be provided"
    assert namespace_and_instruction_args.namespace_list is not None, "namespaces must be provided"

    if not namespace_and_instruction_args.cluster_list:
        print_colored(
            "cluster-prefix/cluster-list not provided. Assuming all nodes are on the current cluster",
            Colors.RED,
        )

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

    for index, config in enumerate(configs):
        if not restarter.restart_service(index):
            print_colored("\nAborting restart process.")
            sys.exit(1)

    print_colored("\nAll pods have been successfully restarted!", Colors.GREEN)

    print("\nOperation completed successfully!")
