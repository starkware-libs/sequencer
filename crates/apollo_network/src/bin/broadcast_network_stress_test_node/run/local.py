import argparse
from multiprocessing.dummy import Process
import os
import subprocess
from time import sleep
import time
import docker
from utils import (
    make_multi_address,
    run_cmd,
    pr,
    get_peer_id_from_node_id,
    project_root,
    make_timestamp,
)
from yaml_maker import get_prometheus_config
from args import add_shared_args_to_parser, get_arguments, get_env_vars
from cluster_start import make_image_tag, build_image


def check_docker():
    pr("Checking if Docker works...")
    run_cmd(
        "docker run --name hello-world hello-world",
        hint="Make sure you have Docker installed and running.",
    )
    run_cmd("docker rm hello-world")
    pr("Docker is working correctly.")


class ExperimentRunner:

    def __init__(
        self,
    ):
        """
        Prometheus logging does not work without docker because the container cannot make make
        requests to scrap the processes that are running...
        """
        self.client = docker.from_env()
        self.prometheus_url = "http://localhost:9090"
        self.grafana_url = "http://localhost:3000"
        self.prometheus_self_scrape = False  # If true, Prometheus will scrape itself
        self.docker_containers = []
        self.running_processes: list[subprocess.Popen] = []
        self.metric_ports = []
        self.docker_image_tag = None
        self.bootstrap_multi_address = ""
        self.tmp_dir_name = "experiment_runner"
        self.timestamp = make_timestamp()
        self.tmp_dir = f"/tmp/broadcast-network-stress-test-{self.timestamp}"
        self.metric_port_base = 2000
        self.p2p_port_base = 10000

    def __enter__(self):
        print("Starting ExperimentRunner...")
        os.makedirs(self.tmp_dir)
        pr(f"Using temporary directory: {self.tmp_dir}")
        return self

    def __exit__(self, exc_type, exc_value, traceback):
        print("Stopping ExperimentRunner...")
        for cont in self.docker_containers:
            pr(f"Stopping container {cont}...")
            cont.stop()
            pr(f"Removing container {cont}...")
            cont.remove()
        self.docker_containers.clear()

        pr("Stopping broadcast_network_stress_test_node nodes...")
        for p in self.running_processes:
            pr(f"Stopping process {p}...")
            p.kill()
        self.running_processes.clear()

    def write_prometheus_config(self) -> str:
        config = get_prometheus_config(
            self_scrape=self.prometheus_self_scrape,
            metric_urls=[f"localhost:{port}" for _, port in self.metric_ports],
        )
        prometheus_config_path = os.path.join(self.tmp_dir, "prometheus.yml")
        pr(f"Writing Prometheus configuration to {prometheus_config_path}...")
        with open(prometheus_config_path, "w") as f:
            f.write(config)
        return prometheus_config_path

    def generate_grafana_dashboard(self) -> str:
        """Generate Grafana dashboard JSON using static configuration."""
        pr("Generating Grafana dashboard configuration...")
        from grafana_config import get_grafana_dashboard_json

        dashboard_json = get_grafana_dashboard_json()
        dashboard_path = os.path.join(self.tmp_dir, "dashboard.json")
        with open(dashboard_path, "w") as f:
            f.write(dashboard_json)

        pr(f"Dashboard configuration saved to {dashboard_path}")
        return dashboard_path

    def write_grafana_datasource_config(self) -> str:
        """Write Grafana datasource configuration."""
        from grafana_config import get_grafana_datasource_config

        datasource_config = get_grafana_datasource_config()
        datasource_path = os.path.join(self.tmp_dir, "datasource.yml")
        with open(datasource_path, "w") as f:
            f.write(datasource_config)
        return datasource_path

    def write_grafana_dashboard_config(self, dashboard_path: str) -> str:
        """Write Grafana dashboard provisioning configuration."""
        from grafana_config import get_grafana_dashboard_provisioning_config

        dashboard_config = get_grafana_dashboard_provisioning_config()
        config_path = os.path.join(self.tmp_dir, "dashboard_config.yml")
        with open(config_path, "w") as f:
            f.write(dashboard_config)
        return config_path

    def run_grafana(self):
        """Run Grafana container with provisioned dashboard."""
        pr("Running Grafana...")

        # Generate dashboard and config files
        dashboard_path = self.generate_grafana_dashboard()
        datasource_path = self.write_grafana_datasource_config()
        dashboard_config_path = self.write_grafana_dashboard_config(dashboard_path)

        # Remove existing Grafana container
        run_cmd("docker rm -f grafana_network_stress_test", may_fail=True)

        # Generate Grafana configuration
        from grafana_config import get_grafana_config, get_grafana_preferences_json

        grafana_config = get_grafana_config()
        grafana_config_path = os.path.join(self.tmp_dir, "grafana.ini")
        with open(grafana_config_path, "w") as f:
            f.write(grafana_config)

        # Generate Grafana preferences
        preferences_json = get_grafana_preferences_json()
        preferences_path = os.path.join(self.tmp_dir, "preferences.json")
        with open(preferences_path, "w") as f:
            f.write(preferences_json)

        # Create Grafana container with provisioned dashboard and datasource
        cont = self.client.containers.run(
            image="grafana/grafana:latest",
            detach=True,
            name="grafana_network_stress_test",
            network="host",
            environment={
                "GF_PATHS_CONFIG": "/etc/grafana/grafana.ini",
            },
            volumes={
                grafana_config_path: {
                    "bind": "/etc/grafana/grafana.ini",
                    "mode": "ro",
                },
                datasource_path: {
                    "bind": "/etc/grafana/provisioning/datasources/datasource.yml",
                    "mode": "ro",
                },
                dashboard_config_path: {
                    "bind": "/etc/grafana/provisioning/dashboards/dashboard_config.yml",
                    "mode": "ro",
                },
                dashboard_path: {
                    "bind": "/etc/grafana/provisioning/dashboards/dashboard.json",
                    "mode": "ro",
                },
            },
            extra_hosts={"host.docker.internal": "host-gateway"},
        )
        self.docker_containers.append(cont)
        pr(f"Grafana available at {self.grafana_url} (no login required)")
        pr(
            f"Direct dashboard link: {self.grafana_url}/d/broadcast-network-stress-test/broadcast-network-stress-test"
        )

    def run_prometheus(self):
        pr("Running Prometheus...")
        prometheus_config_path = self.write_prometheus_config()
        run_cmd("docker rm -f prometheus_network_stress_test", may_fail=True)
        cont = self.client.containers.run(
            image="prom/prometheus",
            detach=True,
            name="prometheus_network_stress_test",
            network="host",
            volumes={
                prometheus_config_path: {
                    "bind": "/etc/prometheus/prometheus.yml",
                    "mode": "ro",
                }
            },
            extra_hosts={"host.docker.internal": "host-gateway"},
        )
        self.docker_containers.append(cont)

    def compile_network_stress_test_node(self, args: argparse.Namespace):
        if args.docker:
            pr("Building Docker image for broadcast_network_stress_test_node...")
            # Build or use existing image
            self.docker_image_tag = (
                args.image if args.image else make_image_tag(self.timestamp)
            )

            if not args.image:
                build_image(self.docker_image_tag)
            else:
                # Check if provided image exists
                try:
                    self.client.images.get(self.docker_image_tag)
                    pr(f"Using existing image: {self.docker_image_tag}")
                except docker.errors.ImageNotFound:
                    raise RuntimeError(
                        f"Specified image '{self.docker_image_tag}' not found. "
                        f"Please build the image first or run without --image to build automatically."
                    )
        else:
            pr("Compiling broadcast_network_stress_test_node node without Docker...")
            run_cmd(
                f'RUSTFLAGS="--cfg tokio_unstable" cargo build --release --bin broadcast_network_stress_test_node',
                hint="Make sure you have Rust and Cargo installed.",
            )

    def run_network_stress_test_node(self, i: int, args: argparse.Namespace):
        pr(f"Running node {i}...")
        assert i >= 0
        metric_port = self.metric_port_base + i
        p2p_port = self.p2p_port_base + i
        exe: str = os.path.abspath(
            f"{project_root()}/target/release/broadcast_network_stress_test_node"
        )

        if args.profile:
            perf_data_file = str(
                os.path.join(
                    self.tmp_dir, f"broadcast_network_stress_test_node{i}.perf.data"
                )
            )
            if args.profile_mode == "cpu":
                arguments = ["perf", "record", "-o", perf_data_file, exe]
            elif args.profile_mode == "mem":
                arguments = [
                    "perf",
                    "mem",
                    "record",
                    "-o",
                    perf_data_file,
                    exe,
                ]
            elif args.profile_mode == "dhat":
                arguments = [
                    "valgrind",
                    "--tool=dhat",
                    f"--dhat-out-file={perf_data_file}.dhat.out",
                    exe,
                ]
            else:
                raise Exception(f"Unrecognized profile mode {args.profile_mode}")
        else:
            arguments = [exe]

        # Generate bootstrap peers for all other nodes using list comprehension
        bootstrap_nodes = [
            make_multi_address(
                network_address="/ip4/127.0.0.1",
                port=self.p2p_port_base + j,
                peer_id=get_peer_id_from_node_id(j),
                args=args,
            )
            for j in range(args.num_nodes)
        ]

        arguments_tuples = get_arguments(
            id=i,
            metric_port=metric_port,
            p2p_port=p2p_port,
            bootstrap_nodes=bootstrap_nodes,
            args=args,
        )
        arguments += [s for pair in arguments_tuples for s in pair]
        pr(f"Running {' '.join(arguments)}")
        # write stdout and stderr to files
        # stdout_file = os.path.join(self.tmp_dir, f"node_{i}_stdout.log")
        # stderr_file = os.path.join(self.tmp_dir, f"node_{i}_stderr.log")
        # with open(stdout_file, "w") as stdout, open(stderr_file, "w") as stderr:
        p = subprocess.Popen(args=arguments)
        self.running_processes.append(p)
        self.metric_ports.append((i, metric_port))

    def run_network_stress_test_node_container(self, i: int, args: argparse.Namespace):
        pr(f"Running node {i} in Docker container...")
        assert i >= 0
        metric_port = self.metric_port_base + i
        p2p_port = self.p2p_port_base + i

        container_name = f"broadcast-network-stress-test-node-{i}"

        # Generate bootstrap peers for all other nodes
        bootstrap_nodes = [
            make_multi_address(
                network_address="/ip4/127.0.0.1",
                port=self.p2p_port_base + j,
                peer_id=get_peer_id_from_node_id(j),
                args=args,
            )
            for j in range(args.num_nodes)
        ]

        # Get command arguments
        env_vars = get_env_vars(
            id=i,
            metric_port=metric_port,
            p2p_port=p2p_port,
            bootstrap_nodes=bootstrap_nodes,
            args=args,
        )

        pr(f"Starting container {container_name}")
        pr(f"Environment variables: {env_vars}")

        # Remove existing container if it exists
        run_cmd(f"docker rm -f {container_name}", may_fail=True)

        # Run the container with network capabilities for traffic control
        cont = self.client.containers.run(
            image=self.docker_image_tag,
            detach=True,
            name=container_name,
            network="host",
            environment={x["name"]: x["value"] for x in env_vars},
            remove=True,
        )

        self.docker_containers.append(cont)
        self.metric_ports.append((i, metric_port))

    def run_network_stress_test_nodes(self, args: argparse.Namespace):
        if args.docker:
            pr(
                "Running broadcast_network_stress_test_node nodes in Docker containers..."
            )
            for i in range(args.num_nodes):
                self.run_network_stress_test_node_container(i, args)
        else:
            pr("Running broadcast_network_stress_test_node nodes without Docker...")
            for i in range(args.num_nodes):
                self.run_network_stress_test_node(i, args=args)

    def check_still_running(self):
        pr("Checking if broadcast_network_stress_test_node nodes are still running...")
        for p in self.running_processes:
            if p.poll() is not None:
                raise Exception(f"Process {p} has stopped.")
        for cont in self.docker_containers:
            cont.reload()
            if cont.status != "running":
                run_cmd(f"docker logs {cont.name}", may_fail=True)
                raise Exception(f"Container {cont.name} has stopped.")

    def run_experiment(self, args: argparse.Namespace):
        check_docker()
        self.compile_network_stress_test_node(args)
        self.run_network_stress_test_nodes(args=args)
        self.run_prometheus()
        self.run_grafana()
        deployment_mode = "Docker containers" if args.docker else "local processes"
        pr(f"Running broadcast_network_stress_test_nodes in {deployment_mode}...")
        pr(f"Visit {self.prometheus_url} to see the metrics.")
        pr(
            f"Visit {self.grafana_url} to see the Grafana dashboard (no login required)."
        )
        pr(
            f"Direct dashboard URL: {self.grafana_url}/d/broadcast-network-stress-test/broadcast-network-stress-test"
        )
        while True:
            self.check_still_running()
            sleep(10)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--profile",
        help="Whether to run perf profiling on each node (files will show up in the tmp directory)",
        action="store_true",
        default=False,
    )
    parser.add_argument(
        "--profile-mode",
        help="The mode to run perf in. Options are 'cpu' and 'mem'.",
        choices=["cpu", "mem", "dhat"],
        default="cpu",
    )
    parser.add_argument(
        "--docker",
        help="Run nodes in Docker containers instead of local processes",
        action="store_true",
        default=False,
    )
    parser.add_argument(
        "--image",
        help="Previously built image tag to use instead of re-building the docker image (only used with --docker)",
        type=str,
        default=None,
    )
    parser.add_argument(
        "--latency",
        help="Min latency to use when gating the network in milliseconds (only used with --docker)",
        type=int,
        default=None,
    )
    parser.add_argument(
        "--throughput",
        help="Max throughput to use when gating the network in KB/s (only used with --docker)",
        type=int,
        default=None,
    )
    add_shared_args_to_parser(parser=parser)
    args = parser.parse_args()
    print(args)

    pr("Starting network stress test experiment...")
    deployment_mode = (
        "Docker containers with network controls" if args.docker else "local processes"
    )
    pr(f"This will run {args.num_nodes} nodes using {deployment_mode}.")

    if args.docker and (args.latency or args.throughput):
        controls = []
        if args.latency:
            controls.append(f"latency: {args.latency}ms")
        if args.throughput:
            controls.append(f"throughput: {args.throughput}KB/s")
        pr(f"Network controls: {', '.join(controls)}")

    with ExperimentRunner() as runner:
        runner.run_experiment(args=args)


if __name__ == "__main__":
    main()
