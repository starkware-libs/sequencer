import argparse
import os
from time import sleep
import json
import time
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
    get_network_stress_test_deployment_yaml_file,
    get_network_stress_test_headless_service_yaml_file,
)
from args import add_shared_args_to_parser


cluster_working_directory: str = os.path.join(
    os.path.expanduser("~"),
    "apollo_broadcast_network_stress_test",
)
network_stress_test_deployment_file_name: str = os.path.join(
    cluster_working_directory, f"network_stress_test_deployment_file.json"
)


def login_to_docker_registry():
    run_cmd("gcloud auth configure-docker us-central1-docker.pkg.dev")


def make_image_tag(timestamp: str) -> str:
    return f"us-central1-docker.pkg.dev/starkware-dev/sequencer/network-stress-test:{timestamp}"


def build_image(image_tag: str):
    dockerfile_path = os.path.abspath("Dockerfile")
    run_cmd(f"docker build -t {image_tag} -f {dockerfile_path} {project_root()}")


def upload_image_to_registry(image_tag: str):
    run_cmd(
        f"docker push {image_tag}",
        hint="Make sure you are logged in to the Docker registry. If so, contact the dev team to resolve any issues (maybe a permissions issue).",
    )


def write_deployment_file(deployment_data: dict):
    with open(network_stress_test_deployment_file_name, "w") as f:
        json.dump(deployment_data, f, indent=4)


def write_yaml_file(file_name: str, file_content: str):
    with open(os.path.join(cluster_working_directory, file_name), "w") as f:
        f.write(file_content)


def write_yaml_files(
    image_tag: str,
    args: argparse.Namespace,
) -> list[str]:
    num_nodes = args.num_nodes
    files = {
        "network-stress-test-deployment.yaml": get_network_stress_test_deployment_yaml_file(
            image_tag, args=args
        ),
        "network-stress-test-headless-service.yaml": get_network_stress_test_headless_service_yaml_file(),
        "prometheus-config.yaml": get_prometheus_yaml_file(num_nodes),
        "prometheus-deployment.yaml": get_prometheus_deployment_yaml_file(),
        "prometheus-service.yaml": get_prometheus_service_yaml_file(),
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
        run_cmd(
            f"kubectl create namespace {name_space_name}",
        )
        self.deployment_file["namespace"] = name_space_name

    def deploy_yaml_files(self, name_space_name: str):
        for file_name in self.deployment_file["yaml_files"]:
            pr(f"Deploying {file_name} to cluster")
            file_path = os.path.join(cluster_working_directory, file_name)
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

        namespace_name = f"network-stress-test-{self.timestamp}"
        self.create_namespace(namespace_name)
        file_names = write_yaml_files(image_tag, args=args)
        self.deployment_file["yaml_files"] = file_names
        self.deploy_yaml_files(namespace_name)

        sleep(10)

        for i in range(args.num_nodes):
            run_cmd(
                f"timeout 5 kubectl logs -n {namespace_name} network-stress-test-{i} > /tmp/network-stress-test-{i}.logs.txt",
                hint=f"Check logs for node {i}",
                may_fail=True,
            )
        run_cmd(
            f"kubectl get pods -n {namespace_name}", hint="Check if pods are running"
        )
        pr("Prometheus deployment complete!")
        pr("To access Prometheus, run: python cluster_port_forward_prometheus.py")
        pr(f"Deployment files saved to: `{network_stress_test_deployment_file_name}`")


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
        "--dedicated-node",
        help="Whether to run the pods on a dedicated node or not",
        action="store_true",
        default=False,
    )
    parser.add_argument(
        "--node-name",
        help="Name of the dedicated node to use (only used if --dedicated-node is set)",
        type=str,
        default="andrew",
    )
    parser.add_argument(
        "--node-role",
        help="Role selector for the dedicated node (only used if --dedicated-node is set)",
        type=str,
        default="andrew",
    )
    # parser.add_argument(
    #     "--timeout-seconds",
    #     help="Maximum duration for the stress test pods to run before automatic termination (seconds)",
    #     type=int,
    #     default=7200,
    # )

    add_shared_args_to_parser(parser=parser)
    args = parser.parse_args()

    assert not os.path.exists(
        network_stress_test_deployment_file_name
    ), "Deployment file already exists. Please run cluster_stop.py before running the experiment."

    with ExperimentRunner() as runner:
        runner.run_experiment(args)


if __name__ == "__main__":
    main()
