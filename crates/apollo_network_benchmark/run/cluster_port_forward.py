#!/usr/bin/env python3
"""
Port forward both Grafana and Prometheus services from Kubernetes cluster to local machine.
"""

import json
import os
import subprocess
import sys
import threading
from cluster_stop import broadcast_network_stress_test_deployment_file_name
from utils import pr, connect_to_cluster
from yaml_maker import prometheus_service_name


def port_forward(service_name, local_port, remote_port, namespace):
    """Run kubectl port-forward for a single service."""
    subprocess.run(
        [
            "kubectl",
            "port-forward",
            f"service/{service_name}",
            f"{local_port}:{remote_port}",
            "-n",
            namespace,
        ]
    )


def main():
    """Set up port forwarding for both Grafana and Prometheus in parallel."""
    if not os.path.exists(broadcast_network_stress_test_deployment_file_name):
        pr("ERROR: No deployment file found. Run cluster_start.py first.")
        sys.exit(1)

    with open(broadcast_network_stress_test_deployment_file_name, "r") as f:
        namespace = json.load(f).get("namespace")

    if not namespace:
        pr("ERROR: No namespace found in deployment file.")
        sys.exit(1)

    connect_to_cluster()

    pr(f"Port forwarding in namespace: {namespace}")
    pr("  → Grafana:    http://localhost:3000")
    pr("  → Prometheus: http://localhost:9090")
    pr("\nPress Ctrl+C to stop\n")

    # Start both port-forwards in separate threads
    threads = [
        threading.Thread(
            target=port_forward,
            args=("grafana-service", 3000, 3000, namespace),
            daemon=True,
        ),
        threading.Thread(
            target=port_forward,
            args=(prometheus_service_name, 9090, 9090, namespace),
            daemon=True,
        ),
    ]

    for t in threads:
        t.start()

    try:
        # Keep main thread alive
        for t in threads:
            t.join()
    except KeyboardInterrupt:
        pr("\nStopping port forwarding...")
        sys.exit(0)


if __name__ == "__main__":
    main()
