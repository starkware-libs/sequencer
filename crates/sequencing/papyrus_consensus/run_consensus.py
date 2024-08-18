import subprocess
import time
import os
import signal
import argparse
import tempfile
import socket
from contextlib import closing
import fcntl

# The SECRET_KEY is used for building the BOOT_NODE_PEER_ID, so they are coupled and must be used together.
SECRET_KEY = "0xabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcd"
BOOT_NODE_PEER_ID = "12D3KooWDFYi71juk6dYWo3UDvqs5gAzGDc124LSvcR5d187Tdvi"

MONITORING_PERIOD = 10


class Node:
    def __init__(self, validator_id, monitoring_gateway_server_port, cmd):
        self.validator_id = validator_id
        self.monitoring_gateway_server_port = monitoring_gateway_server_port
        self.cmd = cmd
        self.process = None
        self.height_and_timestamp = (None, None)  # (height, timestamp)
        self.sync_count = None

    def start(self):
        self.process = subprocess.Popen(self.cmd, shell=True, preexec_fn=os.setsid)

    def stop(self):
        if self.process:
            os.killpg(os.getpgid(self.process.pid), signal.SIGINT)
            self.process.wait()

    def get_metric(self, metric: str):
        port = self.monitoring_gateway_server_port
        command = f"curl -s -X GET http://localhost:{port}/monitoring/metrics | grep -oP '{metric} \\K\\d+'"
        result = subprocess.run(command, shell=True, capture_output=True, text=True)
        return int(result.stdout) if result.stdout else None

    # Check the node's metrics and return the height and timestamp.
    def check_node(self):
        self.sync_count = self.get_metric("papyrus_consensus_sync_count")

        height = self.get_metric("papyrus_consensus_height")
        if self.height_and_timestamp[0] != height:
            if self.height_and_timestamp[0] is not None and height is not None:
                assert height > self.height_and_timestamp[0], "Height should be increasing."
            self.height_and_timestamp = (height, time.time())

        return self.height_and_timestamp


class LockDir:
    def __init__(self, db_dir):
        self.db_dir = db_dir
        self.file_path = os.path.join(db_dir, "lockfile")
        self.file = None

    def __enter__(self):
        self.file = open(self.file_path, "w")
        try:
            fcntl.flock(self.file, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except IOError:
            print(
                f"Could not acquire lock for {self.file_path}, {self.db_dir} is in use by another simulation."
            )
            exit(1)
        return self.file

    def __exit__(self, exc_type, exc_value, traceback):
        if self.file:
            fcntl.flock(self.file, fcntl.LOCK_UN)
            self.file.close()


def find_free_port():
    with closing(socket.socket(socket.AF_INET, socket.SOCK_STREAM)) as s:
        s.bind(("", 0))
        s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEPORT, 1)
        return s.getsockname()[1]


BOOTNODE_TCP_PORT = find_free_port()


# Returns if the simulation should exit.
def monitor_simulation(nodes, start_time, duration, stagnation_timeout):
    curr_time = time.time()
    if duration is not None and duration < (curr_time - start_time):
        return True
    stagnated_nodes = []
    for node in nodes:
        (height, last_update) = node.check_node()
        print(f"Node: {node.validator_id}, height: {height}, sync_count: {node.sync_count}")
        if height is not None and (curr_time - last_update) > stagnation_timeout:
            stagnated_nodes.append(node.validator_id)
    if stagnated_nodes:
        print(f"Nodes {stagnated_nodes} have stagnated. Exiting simulation.")
        return True
    return False


def run_simulation(nodes, duration, stagnation_timeout):
    for node in nodes:
        node.start()

    start_time = time.time()
    try:
        while True:
            time.sleep(MONITORING_PERIOD)
            elapsed = round(time.time() - start_time)
            print(f"\nTime elapsed: {elapsed}s")
            should_exit = monitor_simulation(nodes, start_time, duration, stagnation_timeout)
            if should_exit:
                break
    except KeyboardInterrupt:
        print("\nTerminating subprocesses...")
    finally:
        for node in nodes:
            node.stop()


def build_node(data_dir, logs_dir, i, papryus_args):
    is_bootstrap = i == 1
    tcp_port = BOOTNODE_TCP_PORT if is_bootstrap else find_free_port()
    monitoring_gateway_server_port = find_free_port()
    data_dir = os.path.join(data_dir, f"data{i}")

    cmd = (
        f"RUST_LOG=papyrus_consensus=debug,papyrus=info "
        f"target/release/papyrus_node --network.#is_none false "
        f"--base_layer.node_url {papryus_args.base_layer_node_url} "
        f"--storage.db_config.path_prefix {data_dir} "
        f"--consensus.#is_none false --consensus.validator_id 0x{i} "
        f"--consensus.num_validators {papryus_args.num_validators} "
        f"--network.tcp_port {tcp_port} "
        f"--rpc.server_address 127.0.0.1:{find_free_port()} "
        f"--monitoring_gateway.server_address 127.0.0.1:{monitoring_gateway_server_port} "
        f"--consensus.test.#is_none false "
        f"--consensus.test.cache_size {papryus_args.cache_size} "
        f"--consensus.test.random_seed {papryus_args.random_seed} "
        f"--consensus.test.drop_probability {papryus_args.drop_probability} "
        f"--consensus.test.invalid_probability {papryus_args.invalid_probability} "
        f"--collect_metrics true "
    )

    if is_bootstrap:
        cmd += (
            f"--network.secret_key {SECRET_KEY} "
            + f"2>&1 | sed -r 's/\\x1B\\[[0-9;]*[mK]//g' > {logs_dir}/validator{i}.txt"
        )
    else:
        cmd += (
            f"--network.bootstrap_peer_multiaddr.#is_none false "
            f"--network.bootstrap_peer_multiaddr /ip4/127.0.0.1/tcp/{BOOTNODE_TCP_PORT}/p2p/{BOOT_NODE_PEER_ID} "
            + f"2>&1 | sed -r 's/\\x1B\\[[0-9;]*[mK]//g' > {logs_dir}/validator{i}.txt"
        )

    return Node(
        validator_id=i,
        monitoring_gateway_server_port=monitoring_gateway_server_port,
        cmd=cmd,
    )


def build_all_nodes(data_dir, logs_dir, papryus_args):
    # Validators are started in a specific order to ensure proper network formation:
    # 1. The bootnode (validator 1) is started first for network peering.
    # 2. Validators 2+ are started next to join the network through the bootnode.
    # 3. Validator 0, which is the proposer, is started last so the validators don't miss the proposals.

    nodes = []

    nodes.append(build_node(data_dir, logs_dir, 1, papryus_args))  # Bootstrap

    for i in range(2, papryus_args.num_validators):
        nodes.append(build_node(data_dir, logs_dir, i, papryus_args))

    nodes.append(build_node(data_dir, logs_dir, 0, papryus_args))  # Proposer

    return nodes


# Args passed to the test script that are forwarded to the node.
class PapyrusArgs:
    def __init__(
        self,
        base_layer_node_url,
        num_validators,
        db_dir,
        cache_size,
        random_seed,
        drop_probability,
        invalid_probability,
    ):
        self.base_layer_node_url = base_layer_node_url
        self.num_validators = num_validators
        self.db_dir = db_dir
        self.cache_size = cache_size
        self.random_seed = random_seed
        self.drop_probability = drop_probability
        self.invalid_probability = invalid_probability


# Args passed to the script that are not forwarded to the node.
class RunConsensusArgs:
    def __init__(self, stagnation_threshold, duration):
        self.stagnation_threshold = stagnation_threshold
        self.duration = duration


def main(papyrus_args, run_consensus_args):
    assert (
        papyrus_args.num_validators >= 2
    ), "At least 2 validators are required for the simulation."

    logs_dir = tempfile.mkdtemp()
    db_dir = papyrus_args.db_dir
    if db_dir is not None:
        actual_dirs = {d for d in os.listdir(db_dir) if os.path.isdir(os.path.join(db_dir, d))}
        expected_dirs = {f"data{i}" for i in range(papyrus_args.num_validators)}
        assert expected_dirs.issubset(
            actual_dirs
        ), f"{db_dir} must contain: {', '.join(expected_dirs)}."
    else:
        db_dir = logs_dir
        for i in range(papyrus_args.num_validators):
            os.makedirs(os.path.join(db_dir, f"data{i}"))

    # Acquire lock on the db_dir
    with LockDir(db_dir):
        print("Running cargo build...")
        subprocess.run("cargo build --release --package papyrus_node", shell=True, check=True)

        print(f"DB files will be stored in: {db_dir}")
        print(f"Logs will be stored in: {logs_dir}")

        nodes = build_all_nodes(db_dir, logs_dir, papyrus_args)

        print("Running validators...")
        run_simulation(nodes, run_consensus_args.duration, run_consensus_args.stagnation_threshold)

    print(f"DB files were stored in: {db_dir}")
    print(f"Logs were stored in: {logs_dir}")
    print("Simulation complete.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Run Papyrus Node simulation.")
    parser.add_argument("--base_layer_node_url", required=True)
    parser.add_argument("--num_validators", type=int, required=True)
    parser.add_argument(
        "--db_dir",
        required=False,
        default=None,
        help="Directory with existing DBs that this simulation can reuse.",
    )
    parser.add_argument(
        "--stagnation_threshold",
        type=int,
        required=False,
        default=60,
        help="Time in seconds to check for height stagnation.",
    )
    parser.add_argument("--duration", type=int, required=False, default=None)
    parser.add_argument(
        "--cache_size",
        type=int,
        required=False,
        default=1000,
        help="Cache size for the test simulation.",
    )
    parser.add_argument(
        "--random_seed",
        type=int,
        required=False,
        default=0,
        help="Random seed for test simulation.",
    )
    parser.add_argument(
        "--drop_probability",
        type=float,
        required=False,
        default=0,
        help="Probability of dropping a message for test simulation.",
    )
    parser.add_argument(
        "--invalid_probability",
        type=float,
        required=False,
        default=0,
        help="Probability of sending an invalid message for test simulation.",
    )
    args = parser.parse_args()

    papyrus_args = PapyrusArgs(
        base_layer_node_url=args.base_layer_node_url,
        num_validators=args.num_validators,
        db_dir=args.db_dir,
        cache_size=args.cache_size,
        random_seed=args.random_seed,
        drop_probability=args.drop_probability,
        invalid_probability=args.invalid_probability,
    )

    run_consensus_args = RunConsensusArgs(
        stagnation_threshold=args.stagnation_threshold,
        duration=args.duration,
    )

    main(papyrus_args, run_consensus_args)
