#!/usr/bin/env python3

import argparse
import json
import signal
import socket
import subprocess
import sys
from enum import Enum

from time import sleep
import urllib.error
import urllib.request
from prometheus_client.parser import text_string_to_metric_families

from update_config_and_restart_nodes_lib import (
    ApolloArgsParserBuilder,
    Service,
    get_context_list_from_args,
    get_namespace_list_from_args,
    get_pod_names,
    print_colored,
    print_error,
    restart_node,
    restart_all_nodes,
    restart_pods,
    update_config,
)


class RestartStrategy(Enum):
    """Strategy for restarting nodes."""

    All_At_Once = 1
    One_By_One = 2


def restart_strategy_converter(strategy_name: str) -> RestartStrategy:
    """Convert string to RestartStrategy enum with informative error message"""
    RESTART_STRATEGY_PREFIX = f"{RestartStrategy.__name__}."
    if strategy_name.startswith(RESTART_STRATEGY_PREFIX):
        strategy_name = strategy_name[len(RESTART_STRATEGY_PREFIX) :]

    try:
        return RestartStrategy[strategy_name]
    except KeyError:
        valid_strategies = ", ".join([strategy.name for strategy in RestartStrategy])
        raise argparse.ArgumentTypeError(
            f"Invalid restart strategy '{strategy_name}'. Valid options are: {valid_strategies}"
        )


def get_free_port():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("", 0))
        return s.getsockname()[1]


def get_metrics(port: int, pod: str) -> str:
    for attempt in range(10):
        try:
            with urllib.request.urlopen(f"http://localhost:{port}/monitoring/metrics") as response:
                if response.status == 200:
                    return response.read().decode("utf-8")
                else:
                    print_colored(
                        f"Failed to get metrics from for pod {pod}, attempt {attempt + 1}: {response.status}"
                    )
        except urllib.error.URLError as e:
            print_colored(f"Failed to get metrics from for pod {pod}, attempt {attempt + 1}: {e}")
    print_error(f"Failed to get metrics from for pod {pod}, after {attempt + 1} attempts")
    sys.exit(1)


def poll_until_height_revert(
    local_port: int, pod: str, polling_interval_seconds: int, storage_required_height: int
):
    """Poll metrics until the storage height marker reaches the required height."""
    while True:
        metrics = get_metrics(local_port, pod)
        if metrics is None:
            print_error(f"Failed to get metrics from for pod {pod}")
            sys.exit(1)

        metric_families = text_string_to_metric_families(metrics)
        val = None
        # TODO: change to the real metric (proposal accepted as prposer) when we have a sequencer
        # node (and the metric exists).
        METRIC_NAME = "mempool_pending_queue_size"
        for metric_family in metric_families:
            if metric_family.name == METRIC_NAME:
                val = metric_family.samples[0].value
                break

        if val is None:
            print_colored(
                f"Metric '{METRIC_NAME}' not found in pod {pod}. Assuming the node is not ready."
            )
        else:
            if val < storage_required_height:
                print_colored(
                    f"Storage height marker ({val}) has not reached {storage_required_height} yet, continuing to wait."
                )
            else:
                print_colored(
                    f"Storage height marker ({val}) has reached {storage_required_height}. Safe to continue."
                )
                break

        sleep(polling_interval_seconds)


def wait_for_node(
    pod: str, metrics_port: int, polling_interval_seconds: int, storage_required_height: int
):
    """Wait for the node to be restarted and propose successfully."""
    local_port = get_free_port()
    # Start kubectl port forwarding to the node and keep it running in the background.
    cmd = [
        "kubectl",
        "port-forward",
        f"pod/{pod}",
        f"{local_port}:{metrics_port}",
    ]

    pf_process = None
    try:
        pf_process = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

        # Set up signal handler to ensure subprocess is terminated on interruption
        def signal_handler(signum, frame):
            if pf_process and pf_process.poll() is None:
                print_colored(f"Terminating kubectl port-forward process (PID: {pf_process.pid})")
                pf_process.terminate()
                try:
                    pf_process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    print_colored("Force killing kubectl port-forward process")
                    pf_process.kill()
                    pf_process.wait()
            sys.exit(0)

        # Register signal handlers for graceful shutdown
        signal.signal(signal.SIGINT, signal_handler)
        signal.signal(signal.SIGTERM, signal_handler)

        poll_until_height_revert(local_port, pod, polling_interval_seconds, storage_required_height)

    finally:
        # Ensure subprocess is always terminated
        if pf_process and pf_process.poll() is None:
            print_colored(f"Terminating kubectl port-forward process (PID: {pf_process.pid})")
            pf_process.terminate()
            try:
                pf_process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                print_colored("Force killing kubectl port-forward process")
                pf_process.kill()
                pf_process.wait()


def main():
    usage_example = """
Examples:
  # Restart all nodes to at the next block after current feeder block
  %(prog)s --namespace-prefix apollo-sepolia-integration --num-nodes 3 --feeder_url feeder.integration-sepolia.starknet.io
  %(prog)s -n apollo-sepolia-integration -m 3 -f feeder.integration-sepolia.starknet.io
  
  # Restart nodes with cluster prefix
  %(prog)s -n apollo-sepolia-integration -m 3 -c my-cluster -f feeder.integration-sepolia.starknet.io
  
  # Update configuration without restarting nodes
  %(prog)s -n apollo-sepolia-integration -m 3 -f feeder.integration-sepolia.starknet.io --no-restart
  
  # Restart nodes starting from specific node index
  %(prog)s -n apollo-sepolia-integration -m 3 -s 5 -f feeder.integration-sepolia.starknet.io
  
  # Use different feeder URL
  %(prog)s -n apollo-sepolia-integration -m 3 -f feeder.integration-sepolia.starknet.io
  
  # Use namespace list instead of prefix (restart specific namespaces)
  %(prog)s --namespace-list apollo-sepolia-integration-0 apollo-sepolia-integration-2 -f feeder.integration-sepolia.starknet.io
  %(prog)s -N apollo-sepolia-integration-0 apollo-sepolia-integration-2 -f feeder.integration-sepolia.starknet.io
  
  # Use cluster list for multiple clusters (only works with namespace-list, not namespace-prefix)
  %(prog)s -N apollo-sepolia-integration-0 apollo-sepolia-integration-1 -C cluster1 cluster2 -f feeder.integration-sepolia.starknet.io
  %(prog)s --namespace-list apollo-sepolia-integration-0 apollo-sepolia-integration-1 --cluster-list cluster1 cluster2 -f feeder.integration-sepolia.starknet.io
        """

    args_builder = ApolloArgsParserBuilder(
        "Restart all nodes using the value from the feeder URL", usage_example
    )

    args_builder.add_argument(
        "-f",
        "--feeder_url",
        required=True,
        type=str,
        help="The feeder URL to get the current block from",
    )

    args_builder.add_argument(
        "-r",
        "--restart-strategy",
        type=restart_strategy_converter,
        choices=list(RestartStrategy),
        default=RestartStrategy.All_At_Once,
        help="Strategy for restarting nodes (default: All_At_Once)",
    )

    # The port to connect to to get the metrics.
    args_builder.add_argument(
        "-p",
        "--metrics-port",
        type=int,
        default=8082,
        help="The port to connect to to get the metrics (default: 8082)",
    )

    args = args_builder.build()

    # Get current block number from feeder URL
    try:
        url = f"https://{args.feeder_url}/feeder_gateway/get_block"
        with urllib.request.urlopen(url) as response:
            if response.status != 200:
                raise urllib.error.HTTPError(
                    url, response.status, "HTTP Error", response.headers, None
                )
            data = json.loads(response.read().decode("utf-8"))
            current_block_number = data["block_number"]
            next_block_number = current_block_number + 1

            print_colored(f"Current block number: {current_block_number}")
            print_colored(f"Next block number: {next_block_number}")

    except urllib.error.URLError as e:
        print_error(f"Failed to fetch block number from feeder URL: {e}")
        sys.exit(1)
    except KeyError as e:
        print_error(f"Unexpected response format from feeder URL: {e}")
        sys.exit(1)
    except json.JSONDecodeError as e:
        print_error(f"Failed to parse JSON response from feeder URL: {e}")
        sys.exit(1)

    config_overrides = {
        "consensus_manager_config.immediate_active_height": next_block_number,
        "consensus_manager_config.cende_config.skip_write_height": next_block_number,
    }

    namespace_list = get_namespace_list_from_args(args)
    context_list = get_context_list_from_args(args)
    # update_config(
    #     config_overrides,
    #     namespace_list,
    #     Service.Core,
    #     context_list,
    # )

    if args.no_restart:
        print_colored("\nSkipping pod restart (--no-restart was specified)")
        sys.exit(0)

    if args.restart_strategy == RestartStrategy.One_By_One:
        for index, namespace in enumerate(namespace_list):
            cluster = context_list[index] if context_list else None
            try:
                [pod] = get_pod_names(namespace, Service.Core, cluster)
            except ValueError:
                print_error(f"Expected 1 pod for namespace {namespace}, got: {pod}")
                sys.exit(1)
            # restart_pods(namespace, [pod], index, cluster)
            wait_for_node(pod, args.metrics_port, 5, next_block_number)
    elif args.restart_strategy == RestartStrategy.All_At_Once:
        # restart_all_nodes(
        #     namespace_list,
        #     Service.Core,
        #     context_list,
        # )
        pass
    else:
        print_error(f"Invalid restart strategy: {args.restart_strategy}")
        sys.exit(1)


if __name__ == "__main__":
    main()
