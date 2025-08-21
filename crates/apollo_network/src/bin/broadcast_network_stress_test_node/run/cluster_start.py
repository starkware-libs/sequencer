import argparse
import os
from time import sleep
import json
from cluster_port_forward_prometheus import cluster_port_forward_prometheus
from utils import (
    make_timestamp,
    run_cmd,
    pr,
    connect_to_cluster,
    project_root,
)
from yaml_maker import (
    get_prometheus_yaml_file,
    get_prometheus_deployment_yaml_file,
    get_prometheus_service_yaml_file,
    get_prometheus_headless_service_yaml_file,
    get_network_stress_test_deployment_yaml_file,
    get_network_stress_test_headless_service_yaml_file,
    get_namespace_deletion_job_yaml_file,
    get_namespace_deleter_rbac_yaml_file,
)
from args import add_shared_args_to_parser
from cluster_stop import (
    cluster_working_directory,
    broadcast_network_stress_test_deployment_file_name,
    stop_last_cluster_run,
)


def login_to_docker_registry():
    run_cmd("gcloud auth configure-docker us-central1-docker.pkg.dev")


def make_image_tag(timestamp: str) -> str:
    return f"us-central1-docker.pkg.dev/starkware-dev/sequencer/broadcast-network-stress-test-node:{timestamp}"


def build_image(image_tag: str):
    dockerfile_path = os.path.abspath("Dockerfile")
    run_cmd(f"docker build -t {image_tag} -f {dockerfile_path} {project_root()}")


def upload_image_to_registry(image_tag: str):
    run_cmd(
        f"docker push {image_tag}",
        hint="Make sure you are logged in to the Docker registry. If so, contact the dev team to resolve any issues (maybe a permissions issue).",
    )


def write_deployment_file(deployment_data: dict):
    with open(broadcast_network_stress_test_deployment_file_name, "w") as f:
        json.dump(deployment_data, f, indent=4)


def write_yaml_file(file_name: str, file_content: str):
    with open(os.path.join(cluster_working_directory, file_name), "w") as f:
        f.write(file_content)


def write_yaml_files(
    image_tag: str,
    args: argparse.Namespace,
    namespace_name: str,
    delay_seconds: int,
) -> list[str]:
    num_nodes = args.num_nodes
    files = {
        "broadcast-network-stress-test-deployment.yaml": get_network_stress_test_deployment_yaml_file(
            image_tag, args=args
        ),
        "broadcast-network-stress-test-headless-service.yaml": get_network_stress_test_headless_service_yaml_file(),
        "prometheus-config.yaml": get_prometheus_yaml_file(num_nodes),
        "prometheus-statefulset.yaml": get_prometheus_deployment_yaml_file(),
        "prometheus-service.yaml": get_prometheus_service_yaml_file(),
        "prometheus-headless-service.yaml": get_prometheus_headless_service_yaml_file(),
        "namespace-deleter-rbac.yaml": get_namespace_deleter_rbac_yaml_file(
            namespace_name
        ),
        "namespace-deletion-job.yaml": get_namespace_deletion_job_yaml_file(
            namespace_name, delay_seconds
        ),
    }

    for file_name, file_content in files.items():
        write_yaml_file(file_name, file_content)
    return list(files.keys())


class ExperimentRunner:
    def __enter__(self):
        self.timestamp = make_timestamp()
        run_cmd(f"mkdir -p {cluster_working_directory}")
        self.deployment_file = {"cluster_working_directory": cluster_working_directory}
        return self

    def __exit__(self, exc_type, exc_value, traceback):
        write_deployment_file(self.deployment_file)

    def create_namespace(self, name_space_name: str):
        pr(f"Creating namespace {name_space_name}")
        self.deployment_file["namespace"] = name_space_name
        write_deployment_file(self.deployment_file)
        run_cmd(
            f"kubectl create namespace {name_space_name}",
        )

    def deploy_yaml_files(self, name_space_name: str):
        for file_name in self.deployment_file["yaml_files"]:
            pr(f"Deploying {file_name} to cluster")
            file_path = os.path.join(cluster_working_directory, file_name)

            # RBAC resources and namespace deletion job go to default namespace
            if file_name in [
                "namespace-deleter-rbac.yaml",
                "namespace-deletion-job.yaml",
            ]:
                run_cmd(
                    f"kubectl apply --wait -f {file_path} -n default",
                )
            else:
                run_cmd(
                    f"kubectl apply --wait -f {file_path} -n {name_space_name}",
                )

    def run_experiment(self, args: argparse.Namespace):
        pr(str(args))
        self.deployment_file["args"] = vars(args)
        image_tag = args.image if args.image else make_image_tag(self.timestamp)
        pr(f"timestamp: {self.timestamp}")
        self.deployment_file["timestamp"] = self.timestamp

        if args.image:
            self.deployment_file["was_image_built"] = False
        else:
            pr("Building image")
            build_image(image_tag)
            self.deployment_file["was_image_built"] = True
        pr(f"Image tag: {image_tag}")
        self.deployment_file["image_tag"] = image_tag
        run_cmd(
            f"docker image inspect {image_tag} > /dev/null",
            hint="Make sure the image exists before proceeding.",
        )

        connect_to_cluster()
        login_to_docker_registry()
        upload_image_to_registry(image_tag=image_tag)

        namespace_name = f"broadcast-network-stress-test-{self.timestamp}"
        delay_seconds = args.timeout + 300
        self.deployment_file["delay_seconds"] = delay_seconds

        self.create_namespace(namespace_name)
        file_names = write_yaml_files(
            image_tag,
            args=args,
            namespace_name=namespace_name,
            delay_seconds=delay_seconds,
        )
        self.deployment_file["yaml_files"] = file_names
        self.deploy_yaml_files(namespace_name)

        sleep(10)

        run_cmd(
            f"kubectl get pods -n {namespace_name}", hint="Check if pods are running"
        )
        pr("Prometheus deployment complete!")
        pr("To access Prometheus, run: python cluster_port_forward_prometheus.py")
        pr(
            f"Deployment files saved to: `{broadcast_network_stress_test_deployment_file_name}`"
        )

        # Print namespace deletion info
        pr(
            f"Namespace '{namespace_name}' will be automatically deleted after {self.deployment_file['delay_seconds']} seconds ({args.timeout} + 300)"
        )
        pr("This includes all resources within the namespace.")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--image",
        help="Previously built image tag to use instead of re-building the docker image.",
        type=str,
        default=None,
    )
    parser.add_argument(
        "--latency",
        help="Min latency to use when gating the network in milliseconds.",
        type=int,
        default=None,
    )
    parser.add_argument(
        "--throughput",
        help="Max throughput to use when gating the network in KB/s.",
        type=int,
        default=None,
    )
    parser.add_argument(
        "--dedicated-node-pool",
        help="Whether to run the pods on a dedicated node pool or not",
        action="store_true",
        default=False,
    )
    parser.add_argument(
        "--node-pool-name",
        help="Name of the dedicated node pool to use (only used if --dedicated-node-pool is set)",
        type=str,
        default="andrew",
    )
    parser.add_argument(
        "--node-pool-role",
        help="Role selector for the dedicated node pool (only used if --dedicated-node-pool is set)",
        type=str,
        default="andrew",
    )
    parser.add_argument(
        "--cpu-requests",
        help="CPU requests for each network stress test pod (in Kubernetes format, e.g., '1000m' for 1 core)",
        type=str,
        default="975m",  # running on machine with 3.92 CPU allocatable 3.9 / 4
    )
    parser.add_argument(
        "--memory-requests",
        help="Memory requests for each network stress test pod (in Kubernetes format, e.g., '1Gi' for 1 GiB)",
        type=str,
        default="1Gi",
    )
    parser.add_argument(
        "--cpu-limits",
        help="CPU limit for each network stress test pod (in Kubernetes format, e.g., '1000m' for 1 core)",
        type=str,
        default="975m",  # running on machine with 3.92 CPU allocatable 3.9 / 4
    )
    parser.add_argument(
        "--memory-limits",
        help="Memory limit for each network stress test pod (in Kubernetes format, e.g., '1Gi' for 1 GiB)",
        type=str,
        default="1Gi",
    )

    # parser.add_argument(
    #     "--timeout-seconds",
    #     help="Maximum duration for the stress test pods to run before automatic termination (seconds)",
    #     type=int,
    #     default=7200,
    # )

    add_shared_args_to_parser(parser=parser)
    args = parser.parse_args()

    if os.path.exists(broadcast_network_stress_test_deployment_file_name):
        x = input(
            "Deployment file already exists. Do you want to stop the last run? (y/N): "
        )
        if x.lower() == "y":
            pr("Stopping last cluster run...")
            stop_last_cluster_run()
        else:
            pr("Exiting without running the experiment.")
            return

    assert not os.path.exists(
        broadcast_network_stress_test_deployment_file_name
    ), "Deployment file already exists. Please run cluster_stop.py before running the experiment."

    with ExperimentRunner() as runner:
        runner.run_experiment(args)

    pr("Running cluster_port_forward_prometheus.py")

    cluster_port_forward_prometheus()


if __name__ == "__main__":
    main()
