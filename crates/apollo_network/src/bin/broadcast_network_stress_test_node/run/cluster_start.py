import argparse
import os
from time import sleep
import json
import time
from utils import (
    run_cmd,
    pr,
    network_stress_test_deployment_file_name,
    connect_to_cluster,
    project_root,
    make_time_stamp,
)
from yaml_maker import (
    get_prometheus_yaml_file,
    get_prometheus_deployment_yaml_file,
    get_prometheus_service_yaml_file,
    get_network_stress_test_deployment_yaml_file,
    get_network_stress_test_headless_service_yaml_file,
)
from args import add_shared_args_to_parser


def login_to_docker_registry():
    run_cmd("gcloud auth configure-docker us-central1-docker.pkg.dev")


def make_image_tag(time_stamp: str) -> str:
    return f"us-central1-docker.pkg.dev/starkware-dev/sequencer/network-stress-test:{time_stamp}"


def build_image(image_tag: str):
    dockerfile_path = os.path.abspath("Dockerfile")
    run_cmd(f"docker build -t {image_tag} -f {dockerfile_path} {project_root}")


def upload_image_to_registry(image_tag: str):
    run_cmd(
        f"docker push {image_tag}",
        hint="Make sure you are logged in to the Docker registry. If so, contact the dev team to resolve any issues (maybe a permissions issue).",
    )


def write_deployment_file(deployment_data: dict):
    with open(network_stress_test_deployment_file_name, "w") as f:
        json.dump(deployment_data, f, indent=4)


def write_yaml_file(file_name: str, file_content: str):
    with open(file_name, "w") as f:
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
        self.time_stamp = make_time_stamp()
        self.actual_time_stamp = self.time_stamp
        self.deployment_file = {"actual_time_stamp": self.actual_time_stamp}
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
            run_cmd(
                f"kubectl apply --wait -f {file_name} -n {name_space_name}",
            )

    def run_experiment(self, args: argparse.Namespace):
        self.deployment_file["args"] = vars(args)
        if args.timestamp:
            self.time_stamp = args.timestamp
            pr(f"Using provided timestamp: {self.time_stamp}")
        else:
            pr(f"Using current timestamp: {self.time_stamp}")
        self.deployment_file["time_stamp"] = self.time_stamp

        image_tag = make_image_tag(self.time_stamp)
        if args.timestamp:
            pr(f"Using existing image with tag: {image_tag}")
            self.deployment_file["was_image_built"] = False
        else:
            pr(f"Building image with tag: {image_tag}")
            build_image(image_tag)
            self.deployment_file["was_image_built"] = True
        self.deployment_file["image_tag"] = image_tag

        run_cmd(
            f"docker image inspect {image_tag}",
            hint="Make sure the image exists before proceeding.",
        )

        connect_to_cluster()
        login_to_docker_registry()
        upload_image_to_registry(image_tag=image_tag)

        namespace_name = f"network-stress-test-{self.time_stamp}"
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


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--timestamp",
        help="Previously compiled image timestamp to use instead of re-building the docker image.",
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
    parser.add_argument(
        "--timeout-seconds",
        help="Maximum duration for the stress test pods to run before automatic termination (seconds)",
        type=int,
        default=7200,
    )

    add_shared_args_to_parser(parser=parser)
    args = parser.parse_args()

    assert not os.path.exists(
        network_stress_test_deployment_file_name
    ), "Deployment file already exists. Please run cluster_stop.py before running the experiment."

    with ExperimentRunner() as runner:
        runner.run_experiment(args)


if __name__ == "__main__":
    main()
