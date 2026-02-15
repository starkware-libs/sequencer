import argparse
import os
import random
import socket
import subprocess
import sys
import time
from multiprocessing import Process, Queue
from typing import Any, Dict, List, Optional, Union

import numbers
import requests
from config_loader import find_workspace_root, load_and_merge_configs


def get_services_from_configs(services: List[Dict[str, Any]]) -> List[str]:
    """Extract service names from merged configs."""
    return [s["name"] for s in services]


def get_config_list(service_config: Dict[str, Any]) -> Optional[str]:
    """Extract configList path from merged service config."""
    config = service_config.get("config", {})
    return config.get("configList")


def get_monitoring_endpoint_port(service_config: Dict[str, Any]) -> Union[int, float]:
    """Extract monitoring endpoint port from merged service config."""
    # Check sequencerConfig first (most common location)
    sequencer_config = service_config.get("config", {}).get("sequencerConfig", {})
    port = sequencer_config.get("monitoring_endpoint_config_port")

    if isinstance(port, numbers.Number):
        return port

    # Fallback: check service.ports for monitoring-endpoint port
    print("Fallback: checking service.ports for monitoring-endpoint port")
    service_ports = service_config.get("service", {}).get("ports", [])
    for port_config in service_ports:
        if isinstance(port_config, dict):
            port_name = port_config.get("name", "").lower()
            if "monitoring" in port_name or "monitoring-endpoint" in port_name:
                port_value = port_config.get("port")
                if isinstance(port_value, numbers.Number):
                    return port_value

    raise ValueError(
        f"monitoring_endpoint_config_port not found or not a valid number for service {service_config.get('name', 'unknown')}"
    )


def run(
    cmd: List[str], capture_output: bool = False, check: bool = True, text: bool = True
) -> subprocess.CompletedProcess:
    """
    Run a command and handle errors with detailed output.

    When check=True, always capture output to preserve error details on failure.
    """
    # If check=True, we need to capture output to show errors on failure
    should_capture = capture_output or check

    try:
        result = subprocess.run(cmd, capture_output=should_capture, check=check, text=text)
        return result
    except subprocess.CalledProcessError as e:
        # Print detailed error information
        print(f"❌ Command failed: {' '.join(cmd)}")
        if e.stdout:
            print(f"stdout:\n{e.stdout}")
        if e.stderr:
            print(f"stderr:\n{e.stderr}")
        # Re-raise to maintain original behavior
        raise


def wait_for_port(host: str, port: int, timeout: int = 15) -> bool:
    """Actively wait until a port is open (used to confirm port-forward is ready)."""
    start = time.time()
    while time.time() - start < timeout:
        try:
            with socket.create_connection((host, port), timeout=5):
                return True
        except OSError:
            time.sleep(0.5)
    return False


def check_service_alive(
    address: str,
    timeout: int,
    interval: int,
    initial_delay: int,
    retry: int = 3,
    retry_delay: int = 1,
) -> bool:
    time.sleep(initial_delay)
    start_time = time.time()

    while True:
        elapsed = time.time() - start_time
        if elapsed >= timeout:
            return True

        for attempt in range(1, retry + 1):
            try:
                response = requests.get(address)
                response.raise_for_status()
                break
            except requests.RequestException:
                if attempt == retry:
                    return False
                time.sleep(retry_delay)

        time.sleep(interval)


def run_service_check(
    service_name: str,
    pod_name: str,
    monitoring_port: int,
    offset: int,
    timeout: int,
    interval: int,
    initial_delay: int,
    process_queue: Queue,
    port_forward_retries: int = 3,
    port_forward_retry_delay: int = 2,
    port_wait_timeout: int = 30,
    verbose: bool = False,
):
    pf_process = None
    port_established = False
    local_port = monitoring_port + offset

    # Retry port-forward setup
    for attempt in range(1, port_forward_retries + 1):
        try:
            # Kill previous attempt if exists
            if pf_process:
                try:
                    pf_process.terminate()
                    pf_process.wait(timeout=2)
                except Exception:
                    pass

            print(
                f"[{service_name}] 🚀 Port-forwarding attempt {attempt}/{port_forward_retries} on {local_port} -> {monitoring_port}"
            )

            port_forward_cmd = [
                "kubectl",
                "port-forward",
                pod_name,
                f"{local_port}:{monitoring_port}",
            ]
            if verbose:
                port_forward_cmd.insert(1, "-v=6")
            pf_process = subprocess.Popen(
                port_forward_cmd,
                stdout=subprocess.DEVNULL if not verbose else None,
                stderr=subprocess.DEVNULL if not verbose else None,
            )

            # Wait for port to be ready
            if wait_for_port("127.0.0.1", local_port, timeout=port_wait_timeout):
                port_established = True
                print(f"[{service_name}] ✅ Port-forward established on {local_port}")
                break
            else:
                print(f"[{service_name}] ⚠️ Port-forward attempt {attempt} failed - port not ready")
                if attempt < port_forward_retries:
                    time.sleep(port_forward_retry_delay)

        except Exception as e:
            print(f"[{service_name}] ⚠️ Port-forward attempt {attempt} failed: {e}")
            if attempt < port_forward_retries:
                time.sleep(port_forward_retry_delay)

    if not port_established:
        if pf_process:
            try:
                pf_process.terminate()
                pf_process.wait()
            except Exception:
                pass
        process_queue.put(
            (
                service_name,
                False,
                f"Port-forward did not establish after {port_forward_retries} attempts",
            )
        )
        return

    # Continue with health check
    try:
        address = f"http://localhost:{local_port}/monitoring/alive"
        success = check_service_alive(
            address=address,
            timeout=timeout,
            interval=interval,
            initial_delay=initial_delay,
        )
        if success:
            print(f"[{service_name}] ✅ Passed for {timeout}s")
            process_queue.put((service_name, True, "Health check passed"))
        else:
            print(f"[{service_name}] ❌ Failed health check")
            process_queue.put((service_name, False, "Health check failed"))
    except Exception as e:
        print(f"[{service_name}] 💥 Exception during service check: {e}")
        process_queue.put((service_name, False, str(e)))
    finally:
        if pf_process:
            pf_process.terminate()
            pf_process.wait()


def main(
    services: List[Dict[str, Any]],
    timeout: int,
    interval: int,
    initial_delay: int,
    namespace: str,
    verbose: bool = False,
):
    print(f"Running liveness checks on {len(services)} services")
    print(f"Timeout: {timeout}s")
    print(f"Interval: {interval}s")
    print(f"Initial Delay: {initial_delay}s")

    print("📱 Finding pods for services...")

    process_queue = Queue()
    healthcheck_processes: List[Process] = []

    for offset, service_config in enumerate(services):
        service_name = service_config["name"]
        service_label = f"sequencer-{service_name.lower()}"

        # Small random delay (0-2 seconds) to stagger port-forward starts
        # This reduces conflicts from simultaneous port-forward operations
        if offset > 0:
            delay = random.uniform(0, 2)
            time.sleep(delay)

        try:
            get_cmd = [
                "kubectl",
                "get",
                "pods",
                "-n",
                namespace,
                "-l",
                f"service={service_label}",
                "-o",
                "jsonpath={.items[0].metadata.name}",
            ]
            if verbose:
                get_cmd.insert(1, "-v=6")
            pod_name = run(get_cmd, capture_output=True).stdout.strip()
            print(f"Found pod for {service_name}: {pod_name}")
        except subprocess.CalledProcessError:
            print(f"❌ Missing pod for {service_name}. Aborting!")
            sys.exit(1)

        if not pod_name:
            print(f"❌ No pod found for {service_name}. Aborting!")
            sys.exit(1)

        # Get monitoring port from merged config
        try:
            monitoring_port = get_monitoring_endpoint_port(service_config)
        except ValueError as e:
            print(f"❌ {e}. Aborting!")
            sys.exit(1)

        port_forward_retries = 3
        port_forward_retry_delay = 2
        port_wait_timeout = 30

        process = Process(
            name=service_name,
            target=run_service_check,
            args=(
                service_name,
                pod_name,
                monitoring_port,
                offset,
                timeout,
                interval,
                initial_delay,
                process_queue,
                port_forward_retries,
                port_forward_retry_delay,
                port_wait_timeout,
                verbose,
            ),
        )
        process.start()
        healthcheck_processes.append(process)

    # --- Wait for all healthcheck processes to finish ---
    # Each service runs its healthcheck in its own process.
    # We wait for them to complete, but enforce a timeout to avoid hanging forever.
    results = []
    for process in healthcheck_processes:
        process.join(timeout=timeout + initial_delay + 30)
        if process.is_alive():
            # If a process is still running after the timeout, kill it
            # to prevent CI from running indefinitely.
            print(f"⚠️ Killing hung process {process.pid}")
            process.terminate()
            process.join()

    # --- Collect results from the worker queue ---
    # Each worker reports back exactly one tuple: (service_name, ok_bool, message).
    # We drain everything the workers managed to send.
    while not process_queue.empty():
        results.append(process_queue.get())

    # --- Print results summary and decide overall outcome ---
    print("\n=== RESULTS ===")
    all_ok = True

    # Track which services produced results vs. those that did not
    reported_services = [svc for svc, _, _ in results]
    expected_services = [p.name for p in healthcheck_processes]

    # Print results for all services that reported
    for svc, ok, msg in results:
        status = "✅" if ok else "❌"
        print(f"{status} {svc} - {msg}")
        if not ok:
            all_ok = False

    # Any service with no result is considered failed
    for svc in expected_services:
        if svc not in reported_services:
            print(f"❌ {svc} - No result (process killed or crashed)")
            all_ok = False

    # Fail the CI job if any service failed or failed to report
    if not all_ok:
        sys.exit(1)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Run liveness checks on Kubernetes services (sequencer)."
    )
    parser.add_argument(
        "--layout",
        type=str,
        required=True,
        help="Layout name (e.g., 'hybrid')",
    )
    parser.add_argument(
        "--namespace",
        type=str,
        required=True,
        help="Kubernetes namespace",
    )
    parser.add_argument(
        "--timeout",
        type=int,
        required=True,
        help="Timeout duration in seconds for each service check",
    )
    parser.add_argument(
        "--interval",
        type=int,
        required=True,
        help="Interval between health checks in seconds",
    )
    parser.add_argument(
        "--initial-delay",
        type=int,
        default=int(os.getenv("INITIAL_DELAY_SEC", "10")),
        help="Initial delay before starting health checks (default: env INITIAL_DELAY_SEC or 10)",
    )
    parser.add_argument(
        "--overlay",
        type=str,
        default=None,
        help="Overlay path in dot notation (e.g., 'hybrid.testing.node-0')",
    )
    parser.add_argument(
        "--verbose",
        action="store_true",
        help="Enable verbose kubectl output (adds -v=6 flag to kubectl commands)",
    )

    args = parser.parse_args()

    # Try to find workspace: env var (for CI) > auto-detect
    workspace = os.environ.get("GITHUB_WORKSPACE")
    if not workspace:
        workspace = find_workspace_root()
        if workspace:
            print(f"📁 Auto-detected workspace: {workspace}")

    if not workspace:
        print("❌ Could not determine workspace root.")
        print("   Set GITHUB_WORKSPACE env var or ensure script is in scripts/system_tests/")
        sys.exit(1)

    # Load sequencer configs
    overlay_info = f", overlay={args.overlay}" if args.overlay else ""
    print(f"📋 Loading sequencer configs: layout={args.layout}{overlay_info}")
    merged_services = load_and_merge_configs(
        workspace=workspace, layout=args.layout, overlay=args.overlay
    )

    main(
        services=merged_services,
        timeout=args.timeout,
        interval=args.interval,
        initial_delay=args.initial_delay,
        namespace=args.namespace,
        verbose=args.verbose,
    )
