#!/usr/bin/env python3
"""
Port forward Grafana service from Kubernetes cluster to local machine.
This script reads the deployment information and sets up port forwarding for Grafana.
"""

import json
import os
import signal
import sys
from cluster_stop import broadcast_network_stress_test_deployment_file_name
from utils import run_cmd, pr


def cluster_port_forward_grafana():
    """Set up port forwarding for Grafana service."""
    if not os.path.exists(broadcast_network_stress_test_deployment_file_name):
        pr("No deployment file found. Please run cluster_start.py first.")
        return False

    # Read deployment information
    with open(broadcast_network_stress_test_deployment_file_name, "r") as f:
        deployment_info = json.load(f)

    namespace = deployment_info.get("namespace")
    if not namespace:
        pr("No namespace found in deployment file.")
        return False

    pr(f"Setting up port forwarding for Grafana in namespace: {namespace}")
    pr("Grafana will be available at: http://localhost:3000")
    pr("Default credentials: admin/admin")
    pr("Press Ctrl+C to stop port forwarding")

    try:
        # Set up port forwarding for Grafana
        run_cmd(
            f"kubectl port-forward service/grafana-service 3000:3000 -n {namespace}"
        )
    except KeyboardInterrupt:
        pr("Port forwarding stopped.")
        return True
    except Exception as e:
        pr(f"Failed to set up port forwarding: {e}")
        return False


def main():
    """Main entry point."""
    success = cluster_port_forward_grafana()
    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()
