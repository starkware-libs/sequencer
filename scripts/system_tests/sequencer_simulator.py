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
    max_retries: int = 5,  # Retry the port-forward command itself
):
    """Port-forward with retry logic for transient kubectl connection failures."""

    def is_transient_error(error_msg: str) -> bool:
        """Check if error message indicates a transient connection error."""
        if not error_msg:
            return False
        error_lower = error_msg.lower()
        return any(
            phrase in error_lower
            for phrase in [
                "eof",
                "error dialing backend",
                "error upgrading connection",
                "connection refused",
            ]
        )

    for retry in range(max_retries):
        if retry > 0:
            print(
                f"🔄 Retrying port-forward command (attempt {retry + 1}/{max_retries})...",
                flush=True,
            )
            time.sleep(5)  # Wait before retry

        cmd = ["kubectl", "port-forward", pod_name, f"{local_port}:{remote_port}"]
        process = subprocess.Popen(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )

        error_output_read = False

        def get_error_if_failed():
            """Check if process failed and return error message, None if still running."""
            nonlocal error_output_read
            if error_output_read:
                return None

            if process.poll() is not None:
                error_output_read = True
                try:
                    _, stderr = process.communicate(timeout=1)
                    return stderr.strip() if stderr else "Process terminated with unknown error"
                except subprocess.TimeoutExpired:
                    return "Process terminated but could not read error output"
            return None

        # Give kubectl more time to establish connection in CI
        time.sleep(1.5 if retry == 0 else 0.5)

        # Check if process failed immediately (transient connection error)
        error_msg = get_error_if_failed()
        if error_msg:
            if retry < max_retries - 1 and is_transient_error(error_msg):
                print(
                    f"⚠️  Transient kubectl connection error (will retry): {error_msg[:150]}",
                    flush=True,
                )
                try:
                    process.kill()
                except:
                    pass
                continue  # Retry the port-forward command
            else:
                raise RuntimeError(
                    f"❌ Port-forward to {pod_name}:{remote_port} failed after {retry + 1} attempts.\n"
                    f"kubectl error: {error_msg}"
                )

        if not wait_ready:
            return process

        # Wait for port to be accessible
        for attempt in range(max_attempts):
            # Check if process has failed
            error_msg = get_error_if_failed()
            if error_msg:
                if retry < max_retries - 1 and is_transient_error(error_msg):
                    print(
                        f"⚠️  Transient error during wait (will retry): {error_msg[:150]}",
                        flush=True,
                    )
                    try:
                        process.kill()
                    except:
                        pass
                    break  # Break inner loop to retry outer loop
                else:
                    raise RuntimeError(
                        f"❌ Port-forward to {pod_name}:{remote_port} failed.\n"
                        f"kubectl error: {error_msg}"
                    )

            try:
                with socket.create_connection(("localhost", local_port), timeout=1):
                    print(
                        f"✅ Port-forward to {pod_name}:{remote_port} is ready on localhost:{local_port}",
                        flush=True,
                    )
                    return process
            except Exception:
                if attempt < max_attempts - 1:
                    print(
                        f"🔄 Port-forward to {pod_name}:{remote_port} not ready yet, attempt: {attempt + 1}/{max_attempts}",
                        flush=True,
                    )
                    time.sleep(1)

        # If we get here, port never became ready - retry if we have retries left
        if retry < max_retries - 1:
            print(
                f"⚠️  Port-forward process still running but port not accessible, retrying...",
                flush=True,
            )
            try:
                process.kill()
            except:
                pass
            continue
        else:
            # Final failure
            process.terminate()
            final_error_msg = None
            try:
                process.wait(timeout=2)
                if not error_output_read:
                    try:
                        _, stderr = process.communicate(timeout=1)
                        if stderr:
                            final_error_msg = stderr.strip()
                    except subprocess.TimeoutExpired:
                        pass
            except subprocess.TimeoutExpired:
                process.kill()

            error_details = f"\nkubectl error: {final_error_msg}" if final_error_msg else ""
            raise RuntimeError(
                f"❌ Port-forward to {pod_name}:{remote_port} failed after {max_retries} retries and {max_attempts} attempts.\n"
                f"Port {local_port} is not accessible. Check if the pod is running and the port is correct.\n"
                f"Pod: {pod_name}, Local port: {local_port}, Remote port: {remote_port}{error_details}"
            )

    raise RuntimeError(
        f"❌ Port-forward to {pod_name}:{remote_port} failed after {max_retries} retries"
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
    print(f"📡 Port-forwarding {pod_name} on local port {port}...", flush=True)
    port_forward(pod_name, port, port)

    return port


def main(
    state_sync_monitoring_endpoint_port: int,
    http_server_port: int,
    node_type_str: str,
    sender_address: str,
    receiver_address: str,
):
    print("🚀 Running sequencer simulator....", flush=True)

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
        http_server_service = "Gateway"
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
        f"Running the simulator with http port: {http_server_port} and monitoring port: {state_sync_port}",
        flush=True,
    )
    exit_code = run_simulator(http_server_port, state_sync_port, sender_address, receiver_address)

    if exit_code != 0:
        print("❌ Sequencer simulator failed!", flush=True)
        exit(exit_code)
    else:
        print("✅ Sequencer simulator completed successfully!", flush=True)


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
