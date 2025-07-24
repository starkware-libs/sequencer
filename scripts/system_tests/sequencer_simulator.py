import argparse
import json
import os
import subprocess
from enum import Enum


class NodeType(Enum):
    DISTRIBUTED = "distributed"
    CONSOLIDATED = "consolidated"
    HYBRID = "hybrid"


# TODO(Nadin): Add support for hybrid nodes.
def get_service_label(node_type: NodeType, service: str) -> str:
    if (
        node_type == NodeType.DISTRIBUTED
        or node_type == NodeType.HYBRID
        or node_type == NodeType.CONSOLIDATED
    ):
        return f"sequencer-{service.lower()}"
    else:
        raise ValueError(f"Unknown node type: {node_type}")


def get_config_ports(service_name, deployment_config_path, config_dir, key):
    with open(deployment_config_path, "r", encoding="utf-8") as f:
        deployment_config = json.load(f)

    ports = []
    for service in deployment_config.get("services", []):
        if service.get("name") == service_name:
            for path in service.get("config_paths", []):
                full_path = os.path.join(config_dir, path)
                try:
                    with open(full_path, "r", encoding="utf-8") as cfg_file:
                        config_data = json.load(cfg_file)
                        port = config_data.get(key)
                        print(f"🔍 Found port: {port}")
                        if port:
                            ports.append(port)
                except Exception:
                    continue
    return ports


def get_pod_name(service_label):
    cmd = [
        "kubectl",
        "get",
        "pods",
        "-l",
        f"service={service_label}",
        "-o",
        "jsonpath={.items[0].metadata.name}",
    ]
    return subprocess.run(
        cmd, capture_output=True, check=True, text=True
    ).stdout.strip()


def port_forward(pod_name, local_port, remote_port):
    cmd = ["kubectl", "port-forward", pod_name, f"{local_port}:{remote_port}"]
    subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def run_simulator(http_port, monitoring_port, sender_address, receiver_address):
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
    proc = subprocess.Popen(
        cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True
    )
    with open("sequencer_simulator.log", "w", encoding="utf-8") as log_file:
        for line in proc.stdout:
            print(line, end="")
            log_file.write(line)
    return proc.wait()


def setup_port_forwarding(
    service_name, deployment_config_path, config_dir, config_key, node_type
):
    ports = get_config_ports(
        service_name,
        deployment_config_path,
        config_dir,
        config_key,
    )
    if not ports:
        print(f"❌ No port found for {service_name}! Aborting.")
        exit(1)

    port = ports[-1]
    pod_name = get_pod_name(get_service_label(node_type, service_name))
    print(f"📡 Port-forwarding {pod_name} on local port {port}...")
    port_forward(pod_name, port, port)

    return port


def main(
    deployment_config_path, config_dir, node_type_str, sender_address, receiver_address
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
        deployment_config_path,
        config_dir,
        "monitoring_endpoint_config.port",
        node_type,
    )

    http_server_port = setup_port_forwarding(
        http_server_service,
        deployment_config_path,
        config_dir,
        "http_server_config.port",
        node_type,
    )

    print(
        f"Running the simulator with http port: {http_server_port} and monitoring port: {state_sync_port}"
    )
    exit_code = run_simulator(
        http_server_port, state_sync_port, sender_address, receiver_address
    )

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
        "--deployment_config_path",
        required=True,
        help="Path to the deployment config JSON file.",
    )
    parser.add_argument(
        "--config_dir", required=True, help="Directory containing service config files."
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
        args.deployment_config_path,
        args.config_dir,
        args.node_type,
        args.sender_address,
        args.receiver_address,
    )
