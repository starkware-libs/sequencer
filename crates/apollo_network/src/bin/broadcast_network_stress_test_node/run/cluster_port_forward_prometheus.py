import json
import os
from utils import (
    pr,
    connect_to_cluster,
    run_cmd,
)
from yaml_maker import prometheus_service_name
from cluster_stop import broadcast_network_stress_test_deployment_file_name


def cluster_port_forward_prometheus():
    assert os.path.exists(
        broadcast_network_stress_test_deployment_file_name
    ), "Deployment file does not exist. Have you started a network stress test?"

    with open(broadcast_network_stress_test_deployment_file_name, "r") as f:
        deployment_data: dict = json.load(f)

    name_space_name = deployment_data.get("namespace")
    if name_space_name == None:
        print("ERROR: No namespace found in deployment file")
        return

    connect_to_cluster()

    pr("Access Prometheus at: http://localhost:9090")
    run_cmd(
        f"kubectl port-forward service/{prometheus_service_name} 9090:9090 -n {name_space_name}"
    )


if __name__ == "__main__":
    cluster_port_forward_prometheus()
