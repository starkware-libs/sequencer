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
    # get_network_stress_test_service_yaml_file,
    get_network_stress_test_headless_service_yaml_file,
)


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


def write_yaml_files(image_tag: str, num_nodes: int, namespace: str) -> list[str]:
    files = {
        "prometheus-config.yaml": get_prometheus_yaml_file(num_nodes),
        "prometheus-deployment.yaml": get_prometheus_deployment_yaml_file(),
        "prometheus-service.yaml": get_prometheus_service_yaml_file(),
        "network-stress-test-deployment.yaml": get_network_stress_test_deployment_yaml_file(
            image_tag,
            num_nodes,
            namespace,
            verbosity=3,
        ),
        # "network-stress-test-service.yaml": get_network_stress_test_service_yaml_file(),
        "network-stress-test-headless-service.yaml": get_network_stress_test_headless_service_yaml_file(),
    }
    for file_name, file_content in files.items():
        write_yaml_file(file_name, file_content)
    return list(files.keys())


class ExperimentRunner:
    def __enter__(self):
        self.time_stamp = make_time_stamp()
        self.deployment_file = {}
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
                f"kubectl apply -f {file_name} -n {name_space_name}",
            )

    def run_experiment(self, args: argparse.Namespace):

        if args.time_stamp:
            self.time_stamp = args.time_stamp
            pr(f"Using provided timestamp: {self.time_stamp}")
        else:
            pr(f"Using current timestamp: {self.time_stamp}")
        self.deployment_file["time_stamp"] = self.time_stamp

        image_tag = make_image_tag(self.time_stamp)
        if args.time_stamp:
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
        file_names = write_yaml_files(
            image_tag, num_nodes=args.num_nodes, namespace=namespace_name
        )
        self.deployment_file["yaml_files"] = file_names
        self.deploy_yaml_files(namespace_name)

        sleep(5)

        # for i in range(args.num_nodes):
        #     run_cmd(
        #         f"kubectl logs -n {namespace_name} network-stress-test-{i}",
        #         hint=f"Check logs for node {i}",
        #     )

        run_cmd(
            f"kubectl get pods -n {namespace_name}", hint="Check if pods are running"
        )
        pr("Prometheus deployment complete!")
        pr("To access Prometheus, run: python cluster_port_forward_prometheus.py")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--num-nodes", help="Number of nodes to run", type=int, default=3
    )
    parser.add_argument(
        "--time-stamp",
        help="Previously compiled image timestamp to use instead of re-building the docker image.",
        type=str,
        default=None,
    )
    args = parser.parse_args()

    assert not os.path.exists(
        network_stress_test_deployment_file_name
    ), "Deployment file already exists. Please run cluster_stop.py before running the experiment."

    with ExperimentRunner() as runner:
        runner.run_experiment(args)


if __name__ == "__main__":
    main()
