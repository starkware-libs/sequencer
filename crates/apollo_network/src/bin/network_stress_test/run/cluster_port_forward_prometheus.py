import json
import os
from utils import (
    network_stress_test_deployment_file_name,
    run_cmd,
    pr,
    prometheus_service_name,
    connect_to_cluster,
)


def main():
    assert os.path.exists(
        network_stress_test_deployment_file_name
    ), "Deployment file does not exist. Have you started a network stress test?"

    with open(network_stress_test_deployment_file_name, "r") as f:
        deployment_data: dict = json.load(f)

    name_space_name = deployment_data.get("namespace")
    if name_space_name == None:
        print("ERROR: No namespace found in deployment file")
        return

    # Set up port forwarding
    pr(f"Setting up port forwarding for Prometheus in namespace: {name_space_name}")
    pr("Access Prometheus at: http://localhost:9090")
    pr("Press Ctrl+C to stop port forwarding")

    connect_to_cluster()
    run_cmd(
        f"kubectl port-forward service/{prometheus_service_name} 9090:9090 -n {name_space_name}"
    )


if __name__ == "__main__":
    main()
