import argparse
from multiprocessing.dummy import Process
import os
import subprocess
from time import sleep
import docker
from utils import run_cmd, pr, bootstrap_peer_id, project_root
from yaml_maker import get_prometheus_config


class ExperimentRunner:

    def __init__(
        self,
        use_docker,
    ):
        """
        Prometheus logging does not work without docker because the container cannot make make
        requests to scrap the processes that are running...
        """
        self.client = None
        self.prometheus_url = ""
        self.prometheus_self_scrape = False  # If true, Prometheus will scrape itself
        self.docker_containers = []
        self.running_processes: list[subprocess.Popen] = []
        self.metric_ports = []
        self.docker_image_tag = None
        self.use_docker = use_docker
        self.bootstrap_multi_address = ""
        self.tmp_dir_name = "experiment_runner"
        self.tmp_dir = f"/tmp/{self.tmp_dir_name}"
        self.docker_network = None

    def __enter__(self):
        print("Starting ExperimentRunner...")
        self.client = docker.from_env()
        if not os.path.exists(self.tmp_dir):
            os.makedirs(self.tmp_dir)
        else:
            run_cmd(f"rm -rf /tmp/{self.tmp_dir_name}")
            os.makedirs(self.tmp_dir)
        pr(f"Using temporary directory: {self.tmp_dir}")

        for net in self.client.networks.list(names=["network_stress_test_network"]):
            pr(f"Removing existing Docker network {net.name}...")
            # net.disconnect("", force=True)
            net.reload()
            for cont in net.containers:  # Disconnect all containers from the network
                pr(f"Disconnecting container {cont.name} from network {net.name}...")
                cont.stop()
                cont.remove(force=True)
            net.remove()

        if self.use_docker:
            self.docker_network = self.client.networks.create(
                "network_stress_test_network",
                driver="bridge",
                check_duplicate=True,
                attachable=True,
            )
        else:
            self.docker_network = self.client.networks.get("host")
        return self

    def __exit__(self, exc_type, exc_value, traceback):
        print("Stopping ExperimentRunner...")
        for cont in self.docker_containers:
            pr(f"Stopping container {cont}...")
            cont.stop()
            pr(f"Removing container {cont}...")
            cont.remove()
        self.docker_containers.clear()

        pr("Stopping network_stress_test nodes...")
        for p in self.running_processes:
            pr(f"Stopping process {p}...")
            p.kill()
        self.running_processes.clear()

        if self.use_docker and self.docker_network:
            pr(f"Removing Docker network {self.docker_network.name}...")
            self.docker_network.remove()
            self.docker_network = None

    def check_docker(self):
        pr("Checking if Docker works...")
        cont = self.client.containers.run(
            "hello-world",
            detach=True,
            remove=True,
        )
        code = cont.wait()
        if code["StatusCode"] != 0:
            raise RuntimeError(
                f"Docker hello-world container failed with exit code {code['StatusCode']}"
            )
        pr("Docker is working correctly.")

    def write_prometheus_config(self) -> str:
        config = get_prometheus_config(
            self_scrape=self.prometheus_self_scrape,
            metric_urls=[
                f"http://localhost:{port}/metrics" for _, port in self.metric_ports
            ],
        )

        prometheus_config_path = os.path.join(self.tmp_dir, "prometheus.yml")
        pr(f"Writing Prometheus configuration to {prometheus_config_path}...")
        location = "localhost"
        with open(prometheus_config_path, "w") as f:
            f.write(f"global:\n")
            f.write("  scrape_interval: 1s\n")
            f.write("scrape_configs:\n")
            if self.prometheus_self_scrape:
                f.write(f"  - job_name: prometheus\n")
                f.write(f"    static_configs:\n")
                f.write(f"      - targets: ['{location}:9090']\n")
            for i, port in self.metric_ports:
                f.write(f"  - job_name: 'network_stress_test_{i}'\n")
                f.write(f"    static_configs:\n")
                f.write(f"      - targets: ['{location}:{port}']\n")
                f.write(f"        labels:\n")
                f.write(f"          application: 'network_stress_test'\n")
                f.write(f"          environment: 'test'\n")
        return prometheus_config_path

    def run_prometheus(self):
        pr("Running Prometheus...")
        prometheus_config_path = self.write_prometheus_config()

        cont = self.client.containers.run(
            image="prom/prometheus",
            detach=True,
            name="prometheus_network_stress_test",
            ports={"9090/tcp": 9090} if self.use_docker else None,
            network=self.docker_network.name,
            volumes={
                prometheus_config_path: {
                    "bind": "/etc/prometheus/prometheus.yml",
                    "mode": "ro",
                }
            },
            extra_hosts={"host.docker.internal": "host-gateway"},
        )
        self.docker_containers.append(cont)
        self.prometheus_url = "http://localhost:9090"

    def compile_network_stress_test_node(self):
        if self.use_docker:
            pr("Compiling Docker image...")
            dockerfile_path = os.path.abspath("Dockerfile")
            build_context_path = self.project_root
            pr(
                f"Building Docker image using Dockerfile: {dockerfile_path} with context: {build_context_path}"
            )
            tag = "network_stress_test_node"
            run_cmd(f"docker build -t {tag} -f {dockerfile_path} {build_context_path}")
            self.docker_image_tag = tag
        else:
            pr("Compiling network_stress_test node without Docker...")
            run_cmd(
                "cargo build --release --bin network_stress_test",
                hint="Make sure you have Rust and Cargo installed.",
            )

    # def generate_ed25519_public_key_from_private_key(self, private_key: int) -> str:
    #     pr(f"Generating ED25519 public key from private key {private_key}...")
    #     # write bytes to input file
    #     bytes = private_key.to_bytes(32, byteorder="little")
    #     hex_str = f"{bytes.hex():0>{64}s}"
    #     result = subprocess.run(
    #         [
    #             "cargo",
    #             "run",
    #             "--bin",
    #             "get_peer_id_from_secret_key",
    #             "--",
    #             f"0x{hex_str}",
    #         ],
    #         capture_output=True,
    #         text=True,
    #         check=True,
    #     )
    #     assert result.returncode == 0
    #     result = result.stdout.split()[-1]
    #     pr(f"Generated public key: {result}")
    #     return result

    def run_with_args(
        self, id: int, args: list[str], metric_port: int, p2p_port: int, i: int
    ):
        if self.use_docker:
            cont = self.client.containers.run(
                image=self.docker_image_tag,
                detach=True,
                network=self.docker_network.name,
                name=f"network_stress_test_node_{id}",
                ports={f"{metric_port}/tcp": metric_port, f"{p2p_port}/udp": p2p_port},
                command=args,
                extra_hosts={"host.docker.internal": "host-gateway"},
            )
            self.docker_containers.append(cont)

        else:
            exe = os.path.abspath(f"{project_root}/target/release/network_stress_test")
            if not os.path.exists(exe):
                raise RuntimeError(
                    f"Executable {exe} does not exist. Please compile the project first."
                )
            args.insert(0, exe)
            p = subprocess.Popen(args=args)
            self.running_processes.append(p)

    def run_network_stress_test_node(self, i: int, args: argparse.Namespace):
        pr(f"Running node {i}...")
        assert i >= 0
        metric_port = 2000 + i
        p2p_port = 10000 + i
        args = [
            "--metric-port",
            str(metric_port),
            "--p2p-port",
            str(p2p_port),
            "--id",
            str(i),
            "--verbosity",
            str(args.verbosity),
        ]
        if i != 0:
            args.append("--bootstrap")
            args.append(str(self.bootstrap_multi_address))

        self.run_with_args(i, args, metric_port, p2p_port, i)

        self.metric_ports.append((i, metric_port))
        if i == 0:
            self.bootstrap_multi_address = (
                f"/ip4/127.0.0.1/udp/{p2p_port}/quic-v1/p2p/{bootstrap_peer_id}"
            )

    def run_network_stress_test_nodes(self, args: argparse.Namespace):
        pr("Running network_stress_test nodes without Docker...")
        for i in range(args.num_nodes):
            self.run_network_stress_test_node(i, args=args)

    def check_still_running(self):
        pr("Checking if network_stress_test nodes are still running...")
        for p in self.running_processes:
            if p.poll() is not None:
                raise Exception(f"Process {p} has stopped.")
            else:
                # pr(f"Process {p} is still running.")
                pass
        for cont in self.docker_containers:
            try:
                cont.reload()
                if cont.status != "running":
                    run_cmd(f"docker logs {cont.name}", may_fail=True)
                    raise Exception(f"Container {cont.name} has stopped.")
            except docker.errors.NotFound:
                raise Exception(f"Container {cont.name} not found.")

    def run_bootstrap(self):
        pr("Running bootstrap...")

    def run_experiment(self, args: argparse.Namespace):
        self.check_docker()
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
    parser.add_argument(
        "--verbosity",
        help="Verbosity level for logging (0: None, 1: ERROR, 2: WARN, 3: INFO, 4: DEBUG, 5..: TRACE)",
        type=int,
        default=2,
    )
    args = parser.parse_args()

    pr("Starting network stress test experiment...")
    pr(f"This will run {args.num_nodes} nodes in a local environment without Docker.")
    with ExperimentRunner(use_docker=False) as runner:
        runner.run_experiment(args=args)


if __name__ == "__main__":
    main()
