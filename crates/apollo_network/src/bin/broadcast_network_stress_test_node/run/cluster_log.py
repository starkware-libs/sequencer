from cluster_stop import open_deployment_file
from utils import run_cmd


def cluster_log():
    deployment_data = open_deployment_file()
    num_nodes = deployment_data["args"]["num_nodes"]
    namespace_name = deployment_data["namespace"]

    run_cmd(f"kubectl get pods -n {namespace_name}", hint="Check if pods are running")
    for i in range(num_nodes):
        run_cmd(
            f"timeout 5 kubectl logs -n {namespace_name} broadcast-network-stress-test-{i} > /tmp/broadcast-network-stress-test-{i}.logs.txt",
            hint=f"Check logs for node {i}",
            may_fail=True,
        )
    run_cmd(f"kubectl get pods -n {namespace_name}", hint="Check if pods are running")


if __name__ == "__main__":
    cluster_log()
    print(
        "Cluster logs have been saved to /tmp/broadcast-network-stress-test-*.logs.txt"
    )
