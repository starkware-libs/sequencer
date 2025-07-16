import argparse
from multiprocessing.dummy import Process
import os
import subprocess
from time import sleep
import docker
from utils import (
    run_cmd,
    pr,
    bootstrap_peer_id,
    project_root,
    make_time_stamp,
    check_docker,
    get_prometheus_config,
)
from args import add_broadcast_stress_test_node_arguments_to_parser, get_arguments


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
        self.prometheus_self_scrape = False  # If true, Prometheus will scrape itself
        self.docker_containers = []
        self.running_processes: list[subprocess.Popen] = []
        self.metric_ports = []
        self.docker_image_tag = None
        self.bootstrap_multi_address = ""
        self.tmp_dir_name = "experiment_runner"
        self.tmp_dir = f"/tmp/network-stress-test-{make_time_stamp()}"
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

    def run_prometheus(self):
        pr("Running Prometheus...")
        prometheus_config_path = self.write_prometheus_config()
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

    def compile_network_stress_test_node(self):
        pr("Compiling broadcast_network_stress_test_node node without Docker...")
        run_cmd(
            "cargo build --release --bin broadcast_network_stress_test_node",
            hint="Make sure you have Rust and Cargo installed.",
        )

    def run_network_stress_test_node(self, i: int, args: argparse.Namespace):
        pr(f"Running node {i}...")
        assert i >= 0
        metric_port = self.metric_port_base + i
        p2p_port = self.p2p_port_base + i
        exe: str = os.path.abspath(
            f"{project_root}/target/release/broadcast_network_stress_test_node"
        )
        arguments = [exe]
        arguments += get_arguments(
            id=i,
            metric_port=metric_port,
            p2p_port=p2p_port,
            bootstrap=f"/ip4/127.0.0.1/udp/{self.p2p_port_base}/quic-v1/p2p/{bootstrap_peer_id}",
            args=args,
        )
        p = subprocess.Popen(args=arguments)
        self.running_processes.append(p)
        self.metric_ports.append((i, metric_port))

    def run_network_stress_test_nodes(self, args: argparse.Namespace):
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
        self.compile_network_stress_test_node()
        self.run_network_stress_test_nodes(args=args)
        self.run_prometheus()
        pr("Running network_stress_test_nodes...")
        pr(f"Visit {self.prometheus_url} to see the metrics.")
        while True:
            self.check_still_running()
            sleep(10)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--num-nodes", help="Number of nodes to run", type=int, default=3
    )
    add_broadcast_stress_test_node_arguments_to_parser(parser=parser)
    args = parser.parse_args()

    pr("Starting network stress test experiment...")
    pr(f"This will run {args.num_nodes} nodes in a local environment without Docker.")
    with ExperimentRunner() as runner:
        runner.run_experiment(args=args)


if __name__ == "__main__":
    main()
