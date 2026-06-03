#!/usr/bin/env python3

import signal
import socket
import subprocess
import sys
import threading
import urllib.error
import urllib.request
from time import sleep
from typing import Any, Callable, Optional

from common_lib import Colors, get_namespace_args, print_colored, print_error
from prometheus_client.parser import text_string_to_metric_families

# Registry of live `kubectl port-forward` processes, so they can be cleaned up even when `gate()`
# runs on a worker thread (where it cannot install its own signal handlers). Guarded by a lock since
# parallel waits register/unregister concurrently.
_active_port_forwards_lock = threading.Lock()
_active_port_forwards: set[subprocess.Popen] = set()

# Set when port-forwards are torn down by an interrupt handler, so a gate() running on a worker
# thread can tell an intentional shutdown apart from a port-forward that failed to start.
_shutdown_requested = threading.Event()


def terminate_all_port_forwards() -> None:
    """Terminate every port-forward process still running.

    Best-effort cleanup intended to be called from a SIGINT/SIGTERM handler installed on the main
    thread by the orchestrator when waits run in parallel (worker threads cannot install handlers).
    """
    _shutdown_requested.set()
    with _active_port_forwards_lock:
        port_forward_processes = list(_active_port_forwards)
    for port_forward_process in port_forward_processes:
        MetricConditionGater._terminate_port_forward_process(port_forward_process)


class MetricConditionGater:
    """Gates progress on a metric satisfying a condition.

    This class was meant to be used with counter/gauge metrics. It may not work properly with histogram metrics.
    """

    class Metric:
        def __init__(
            self,
            name: str,
            value_condition: Callable[[Any], bool],
            condition_description: Optional[str] = None,
        ):
            self.name = name
            self.value_condition = value_condition
            self.condition_description = condition_description

    def __init__(
        self,
        metric: "MetricConditionGater.Metric",
        namespace: str,
        cluster: Optional[str],
        pod: str,
        metrics_port: int,
        refresh_interval_seconds: int = 3,
    ):
        self.metric = metric
        self.local_port = self._get_free_port()
        self.namespace = namespace
        self.cluster = cluster
        self.pod = pod
        self.metrics_port = metrics_port
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
            f"({self.metric.condition_description}) "
            if self.metric.condition_description is not None
            else ""
        )

        while True:
            metrics = self._get_metrics_raw_string()
            assert metrics is not None, f"Failed to get metrics from for pod {self.pod}"

            metric_families = text_string_to_metric_families(metrics)
            val = None
            for metric_family in metric_families:
                if metric_family.name == self.metric.name:
                    if len(metric_family.samples) > 1:
                        print_error(
                            f"Multiple samples found for metric {self.metric.name}. Using the first one.",
                        )
                    val = metric_family.samples[0].value
                    break

            if val is None:
                print_colored(
                    f"Metric '{self.metric.name}' not found in pod {self.pod}. Assuming the node is not ready."
                )
            elif self.metric.value_condition(val):
                print_colored(
                    f"Metric {self.metric.name} condition {condition_description}met (value={val})."
                )
                return
            else:
                print_colored(
                    f"Metric {self.metric.name} condition {condition_description}not met (value={val}). Continuing to wait."
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
        with _active_port_forwards_lock:
            _active_port_forwards.discard(pf_process)

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
            with _active_port_forwards_lock:
                _active_port_forwards.add(pf_process)
            print_colored("Waiting for forwarding to start")
            # Give the forwarding time to start.
            # TODO(guy.f): Consider poll until the forwarding is ready if we see any issues.
            sleep(3)
            if pf_process.poll() is not None:
                if _shutdown_requested.is_set():
                    # An interrupt handler tore down the port-forward during startup; exit cleanly
                    # rather than treating the intentional shutdown as an unexpected failure.
                    return
                raise RuntimeError(
                    f"Port forwarding process for pod {self.pod} exited with code "
                    f"{pf_process.returncode}"
                )

            print_colored(
                f"Forwarding started (from local port {self.local_port} to {self.pod}:{self.metrics_port})"
            )

            # Set up a signal handler to terminate the forwarding subprocess on interruption.
            # signal.signal only works on the main thread, so skip it when gating runs on a worker
            # thread (parallel waits) — the orchestrator installs a main-thread handler that calls
            # terminate_all_port_forwards instead, and the finally block below still cleans up on
            # normal completion.
            if threading.current_thread() is threading.main_thread():

                def signal_handler(signum, frame):
                    self._terminate_port_forward_process(pf_process)
                    sys.exit(0)

                signal.signal(signal.SIGINT, signal_handler)
                signal.signal(signal.SIGTERM, signal_handler)

            self._poll_until_condition_met()

        finally:
            self._terminate_port_forward_process(pf_process)
