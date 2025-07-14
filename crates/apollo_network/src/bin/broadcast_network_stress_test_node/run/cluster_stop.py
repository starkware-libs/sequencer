import os
import json
from utils import (
    pr,
    run_cmd,
    connect_to_cluster,
)
from cluster_start import (
    cluster_working_directory,
    network_stress_test_deployment_file_name,
)


def main():
    assert os.path.exists(
        network_stress_test_deployment_file_name
    ), "Deployment file does not exist. Have you started a network stress test?"

    with open(network_stress_test_deployment_file_name, "r") as f:
        deployment_data: dict = json.load(f)

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

    assert network_stress_test_deployment_file_name.startswith(
        f"{cluster_working_directory}/"
    )
    run_cmd(f"rm -rf {cluster_working_directory}")
    pr("Network stress test stopped successfully.")


if __name__ == "__main__":
    main()
