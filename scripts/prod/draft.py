#!/usr/bin/env python3

import signal
import socket
import subprocess
import sys

from time import sleep
import urllib.error
import urllib.parse
import urllib.request
from prometheus_client.parser import text_string_to_metric_families

from update_config_and_restart_nodes_lib import (
    ApolloArgsParserBuilder,
    Colors,
    Service,
    get_configmap,
    get_context_list_from_args,
    get_current_block_number,
    get_logs_explorer_url,
    get_namespace_list_from_args,
    print_colored,
    print_error,
)


# TODO(guy.f): Remove this once we have metrics we use to decide based on.
def get_logs_explorer_url_for_proposal(
    namespace: str,
    validator_id: str,
    min_block_number: int,
    project_name: str,
) -> str:
    # Remove the 0x prefix from the validator id to get the number.
    validator_id = validator_id[2:]

    query = (
        f'resource.labels.namespace_name:"{urllib.parse.quote(namespace)}"\n'
        f'resource.labels.container_name="sequencer-core"\n'
        f'textPayload =~ "DECISION_REACHED:.*proposer 0x0*{validator_id}"\n'
        f'CAST(REGEXP_EXTRACT(textPayload, "height: (\\\\d+)"), "INT64") > {min_block_number}'
    )
    return get_logs_explorer_url(query, project_name)


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

    print(cmd)

    pf_process = None
    try:
        pf_process = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        print("Waiting for forwarding to start")
        sleep(3)  # Give the forwarding time to start. TODO: Replace with polling.

        print("Forwarding started")

        # Set up signal handler to ensure subprocess is terminated on interruption
        def signal_handler(signum, frame):
            if pf_process and pf_process.poll() is None:
                print_colored(
                    f"Terminating kubectl port-forward process (PID: {pf_process.pid})", Colors.RED
                )
                pf_process.terminate()
                try:
                    pf_process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    print_colored("Force killing kubectl port-forward process", Colors.RED)
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
        """

    args_builder = ApolloArgsParserBuilder(
        "Restart all nodes using the value from the feeder URL",
        usage_example,
        include_restart_strategy=False,
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

    namespace_list = get_namespace_list_from_args(args)
    context_list = get_context_list_from_args(args)
    if context_list is not None:
        assert len(namespace_list) == len(
            context_list
        ), "namespace_list and context_list must have the same length"

    wait_for_node(namespace_list[0], args.metrics_port, 3, 1)


if __name__ == "__main__":
    main()
