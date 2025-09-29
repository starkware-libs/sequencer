import os
import json
from utils import (
    pr,
    run_cmd,
    connect_to_cluster,
)

cluster_working_directory: str = os.path.join(
    os.path.expanduser("~"),
    "apollo_broadcast_network_stress_test",
)
broadcast_network_stress_test_deployment_file_name: str = os.path.join(
    cluster_working_directory, f"broadcast_network_stress_test_deployment_file.json"
)


def open_deployment_file() -> dict:
    assert os.path.exists(
        broadcast_network_stress_test_deployment_file_name
    ), "Deployment file does not exist. Have you started a network stress test?"

    with open(broadcast_network_stress_test_deployment_file_name, "r") as f:
        deployment_data: dict = json.load(f)

    return deployment_data


def stop_last_cluster_run():
    deployment_data = open_deployment_file()
    name_space_name = deployment_data.get("namespace")
    if name_space_name != None:
        connect_to_cluster()
        # remove and re-create the namespace to ensure a clean state
        # from <https://stackoverflow.com/questions/47128586/how-to-delete-all-resources-from-kubernetes-one-time>
        run_cmd(f"kubectl delete namespace {name_space_name}", may_fail=True)
        run_cmd(
            f"kubectl create namespace {name_space_name}",
        )
        run_cmd(
            f"kubectl delete namespace {name_space_name}",
        )

    assert broadcast_network_stress_test_deployment_file_name.startswith(
        f"{cluster_working_directory}/"
    )
    run_cmd(f"rm -rf {cluster_working_directory}")
    pr("Network stress test stopped successfully.")


if __name__ == "__main__":
    stop_last_cluster_run()
