import argparse
import subprocess
import time
from enum import Enum

import socket


class NodeType(Enum):
    DISTRIBUTED = "distributed"
    CONSOLIDATED = "consolidated"
    HYBRID = "hybrid"


def get_service_label(node_type: NodeType, service: str) -> str:
    if (
        node_type == NodeType.DISTRIBUTED
        or node_type == NodeType.HYBRID
        or node_type == NodeType.CONSOLIDATED
    ):
        return f"sequencer-{service.lower()}"
    else:
        raise ValueError(f"Unknown node type: {node_type}. Aborting!")


def get_pod_name(service_label: str) -> str:
    cmd = [
        "kubectl",
        "get",
        "pods",
        "-l",
        f"service={service_label}",
        "-o",
        "jsonpath={.items[0].metadata.name}",
    ]
    return subprocess.run(cmd, capture_output=True, check=True, text=True).stdout.strip()


def port_forward(
    pod_name: str,
    local_port: int,
    remote_port: int,
    wait_ready: bool = True,
    max_attempts: int = 25,
):
    cmd = ["kubectl", "port-forward", pod_name, f"{local_port}:{remote_port}"]
    subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if not wait_ready:
        return
    for attempt in range(max_attempts):
        try:
            with socket.create_connection(("localhost", local_port), timeout=1):
                print(
                    f"✅ Port-forward to {pod_name}:{remote_port} is ready on localhost:{local_port}"
                )
                return
        except Exception:
            print(
                f"🔄 Port-forward to {pod_name}:{remote_port} failed, attempt: {attempt}/{max_attempts}"
            )
            time.sleep(1)

    raise RuntimeError(
        f"❌ Port-forward to {pod_name}:{remote_port} failed after {max_attempts} attempts."
    )


def run_simulator(http_port: int, monitoring_port: int, sender_address: str, receiver_address: str):
    cmd = [
        "./target/debug/sequencer_simulator",
        "--http-port",
        str(http_port),
        "--monitoring-port",
        str(monitoring_port),
        "--sender-address",
        sender_address,
        "--receiver-address",
        receiver_address,
    ]
    result = subprocess.run(cmd, check=False)
    return result.returncode


def setup_port_forwarding(service_name: str, port: int, node_type: NodeType):
    pod_name = get_pod_name(get_service_label(node_type, service_name))
    print(f"📡 Port-forwarding {pod_name} on local port {port}...")
    port_forward(pod_name, port, port)

    return port


def main(
    state_sync_monitoring_endpoint_port: int,
    http_server_port: int,
    node_type_str: str,
    sender_address: str,
    receiver_address: str,
):
    print("🚀 Running sequencer simulator....")

    try:
        node_type = NodeType(node_type_str)
    except ValueError:
        print(f"❌ Unknown node type: {node_type_str}.")
        exit(1)

    if node_type == NodeType.DISTRIBUTED:
        state_sync_service = "StateSync"
        http_server_service = "HttpServer"
    elif node_type == NodeType.CONSOLIDATED:
        state_sync_service = "Node"
        http_server_service = "Node"
    elif node_type == NodeType.HYBRID:
        state_sync_service = "Core"
        http_server_service = "HttpServer"
    else:
        print(f"❌ {node_type} node type is not supported for the sequencer simulator.")
        exit(1)

    # Port-forward services
    state_sync_port = setup_port_forwarding(
        state_sync_service,
        state_sync_monitoring_endpoint_port,
        node_type,
    )

    http_server_port = setup_port_forwarding(
        http_server_service,
        http_server_port,
        node_type,
    )

    print(
        f"Running the simulator with http port: {http_server_port} and monitoring port: {state_sync_port}"
    )
    exit_code = run_simulator(http_server_port, state_sync_port, sender_address, receiver_address)

    if exit_code != 0:
        print("❌ Sequencer simulator failed!")
        exit(exit_code)
    else:
        print("✅ Sequencer simulator completed successfully!")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Run the Sequencer Simulator with port forwarding."
    )
    parser.add_argument(
        "--state_sync_monitoring_endpoint_port",
        required=True,
        help="State Sync Monitoring endpoint port.",
    )
    parser.add_argument(
        "--http_server_port",
        required=True,
        help="Http server port.",
    )
    parser.add_argument(
        "--node_type",
        choices=[node_type.value for node_type in NodeType],
        required=True,
        help="Type of node to deploy: 'distributed' or 'consolidated'.",
    )
    parser.add_argument(
        "--sender_address",
        required=True,
        help="Ethereum sender address (e.g., 0xabc...).",
    )
    parser.add_argument(
        "--receiver_address",
        required=True,
        help="Ethereum receiver address (e.g., 0xdef...).",
    )

    args = parser.parse_args()

    main(
        args.state_sync_monitoring_endpoint_port,
        args.http_server_port,
        args.node_type,
        args.sender_address,
        args.receiver_address,
    )
