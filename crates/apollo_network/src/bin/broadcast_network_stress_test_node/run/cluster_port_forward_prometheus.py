import json
import os
import signal
import subprocess
import sys
import time
from utils import (
    pr,
    connect_to_cluster,
)
from yaml_maker import prometheus_service_name
from cluster_stop import broadcast_network_stress_test_deployment_file_name


class PortForwardManager:
    def __init__(self, namespace: str, service: str, local_port: int, remote_port: int):
        self.namespace = namespace
        self.service = service
        self.local_port = local_port
        self.remote_port = remote_port
        self.process = None
        self.running = True
        self.retry_count = 0
        self.max_retries = 5
        self.retry_delay = 2

    def _setup_signal_handlers(self):
        """Set up signal handlers for graceful shutdown."""

        def signal_handler(signum, frame):
            pr("Received interrupt signal. Shutting down gracefully...")
            self.running = False
            if self.process:
                self.process.terminate()
            sys.exit(0)

        signal.signal(signal.SIGINT, signal_handler)
        signal.signal(signal.SIGTERM, signal_handler)

    def _start_port_forward(self):
        """Start the kubectl port-forward process."""
        cmd = [
            "kubectl",
            "port-forward",
            f"service/{self.service}",
            f"{self.local_port}:{self.remote_port}",
            "-n",
            self.namespace,
        ]

        pr(f"Starting port forward: {' '.join(cmd)}")

        try:
            self.process = subprocess.Popen(
                cmd,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                bufsize=1,
                universal_newlines=True,
            )
            return True
        except Exception as e:
            pr(f"Failed to start port forwarding: {e}")
            return False

    def _monitor_process(self):
        """Monitor the port forwarding process and handle output."""
        if not self.process:
            return False

        # Check if process is still running
        if self.process.poll() is not None:
            stdout, stderr = self.process.communicate()
            if stderr or "error" in stdout.lower():
                pr(f"Port forwarding error: {stderr}")
            return False

        return True

    def run_with_retry(self):
        """Run port forwarding with automatic retry on failure."""
        self._setup_signal_handlers()

        pr(
            f"Setting up resilient port forwarding for {self.service} in namespace: {self.namespace}"
        )
        pr(f"Access Prometheus at: http://localhost:{self.local_port}")
        pr("Press Ctrl+C to stop port forwarding")
        pr("Connection will automatically retry on failure...")

        while self.running and self.retry_count < self.max_retries:
            if self._start_port_forward():
                pr(
                    f"Port forwarding established successfully (attempt {self.retry_count + 1})"
                )
                self.retry_count = 0  # Reset retry count on successful connection

                # Monitor the connection
                try:
                    while self.running and self._monitor_process():
                        time.sleep(1)
                except KeyboardInterrupt:
                    break

                if self.running:  # Only retry if we're still supposed to be running
                    pr("Port forwarding connection lost. Retrying...")
                    self.retry_count += 1
                    if self.retry_count < self.max_retries:
                        pr(
                            f"Retrying in {self.retry_delay} seconds... (attempt {self.retry_count + 1}/{self.max_retries})"
                        )
                        time.sleep(self.retry_delay)
                        # Exponential backoff
                        self.retry_delay = min(self.retry_delay * 1.5, 10)
            else:
                self.retry_count += 1
                if self.retry_count < self.max_retries:
                    pr(
                        f"Failed to start port forwarding. Retrying in {self.retry_delay} seconds... (attempt {self.retry_count + 1}/{self.max_retries})"
                    )
                    time.sleep(self.retry_delay)
                    self.retry_delay = min(self.retry_delay * 1.5, 10)

        if self.retry_count >= self.max_retries:
            pr(
                f"Failed to establish stable port forwarding after {self.max_retries} attempts"
            )
            return False

        pr("Port forwarding stopped")
        return True


def main():
    assert os.path.exists(
        broadcast_network_stress_test_deployment_file_name
    ), "Deployment file does not exist. Have you started a network stress test?"

    with open(broadcast_network_stress_test_deployment_file_name, "r") as f:
        deployment_data: dict = json.load(f)

    name_space_name = deployment_data.get("namespace")
    if name_space_name == None:
        print("ERROR: No namespace found in deployment file")
        return

    # Connect to cluster first
    connect_to_cluster()

    # Set up resilient port forwarding
    port_forward_manager = PortForwardManager(
        namespace=name_space_name,
        service=prometheus_service_name,
        local_port=9090,
        remote_port=9090,
    )

    port_forward_manager.run_with_retry()


if __name__ == "__main__":
    main()
