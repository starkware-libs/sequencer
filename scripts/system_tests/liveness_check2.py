import argparse
import os
import subprocess
import sys
import time
from pathlib import Path
from typing import Any, Dict, List, Optional, Union

import numbers
import requests
import socket
import yaml
from copy import deepcopy
from multiprocessing import Process, Queue


def load_yaml(file_path: Path) -> Dict[str, Any]:
    """Load a YAML file."""
    if not file_path.exists():
        return {}
    with open(file_path, "r", encoding="utf-8") as f:
        return yaml.safe_load(f) or {}


def deep_merge_dict(base: Dict[str, Any], overlay: Dict[str, Any]) -> Dict[str, Any]:
    """Deep merge overlay dict into base dict."""
    result = deepcopy(base)
    for key, value in overlay.items():
        if key in result and isinstance(result[key], dict) and isinstance(value, dict):
            result[key] = deep_merge_dict(result[key], value)
        else:
            result[key] = value
    return result


def find_workspace_root() -> Optional[str]:
    """
    Auto-detect workspace root: ../.. from script location.

    Script is at: scripts/system_tests/liveness_check2.py
    Repo root is: ../.. from script location
    """
    script_dir = Path(__file__).parent.resolve()
    workspace_root = script_dir.parent.parent.resolve()
    return str(workspace_root)


def load_and_merge_configs(workspace: str, layout: str) -> List[Dict[str, Any]]:
    """
    Load and merge sequencer2 configs (layout + common.yaml).

    Returns a list of merged service configs.
    """
    base_dir = Path(workspace) / "deployments" / "sequencer2"

    # Load layout common.yaml
    layout_common_path = base_dir / "configs" / "layouts" / layout / "common.yaml"
    layout_common = load_yaml(layout_common_path)

    # Load layout service configs
    layout_services_dir = base_dir / "configs" / "layouts" / layout / "services"
    layout_services = {}
    if layout_services_dir.exists():
        for service_file in layout_services_dir.glob("*.yaml"):
            service_config = load_yaml(service_file)
            if "name" in service_config:
                layout_services[service_config["name"]] = service_config

    # Merge common into each service (service is base, common overlays)
    merged_services = []
    for service_name, layout_service in layout_services.items():
        # Start with service as base, then merge common (common can add/modify, service takes precedence)
        merged_service = deep_merge_dict(layout_service, layout_common)

        # Ensure name is set (service name always takes precedence)
        merged_service["name"] = service_name
        merged_services.append(merged_service)

    return merged_services


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
    service_ports = service_config.get("service", {}).get("ports", [])
    for port_config in service_ports:
        if isinstance(port_config, dict):
            port_name = port_config.get("name", "").lower()
            if "monitoring" in port_name:
                port_value = port_config.get("port")
                if isinstance(port_value, numbers.Number):
                    return port_value

    raise ValueError(
        f"monitoring_endpoint_config_port not found or not a valid number for service {service_config.get('name', 'unknown')}"
    )


def run(
    cmd: List[str], capture_output: bool = False, check: bool = True, text: bool = True
) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=capture_output, check=check, text=text)


def wait_for_port(host: str, port: int, timeout: int = 15) -> bool:
    """Actively wait until a port is open (used to confirm port-forward is ready)."""
    start = time.time()
    while time.time() - start < timeout:
        try:
            with socket.create_connection((host, port), timeout=1):
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
    print(f"Initial delay: {initial_delay}s")

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
):
    try:
        local_port = monitoring_port + offset
        print(f"[{service_name}] 🚀 Port-forwarding on {local_port} -> {monitoring_port}")

        pf_process = subprocess.Popen(
            ["kubectl", "port-forward", pod_name, f"{local_port}:{monitoring_port}"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )

        try:
            if not wait_for_port("127.0.0.1", local_port, timeout=15):
                process_queue.put((service_name, False, "Port-forward did not establish"))
                return

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
        finally:
            pf_process.terminate()
            pf_process.wait()
    except Exception as e:
        process_queue.put((service_name, False, str(e)))


def main(
    services: List[Dict[str, Any]],
    timeout: int,
    interval: int,
    initial_delay: int,
    namespace: str,
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

        try:
            pod_name = run(
                [
                    "kubectl",
                    "get",
                    "pods",
                    "-n",
                    namespace,
                    "-l",
                    f"service={service_label}",
                    "-o",
                    "jsonpath={.items[0].metadata.name}",
                ],
                capture_output=True,
            ).stdout.strip()
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
        description="Run liveness checks on Kubernetes services (sequencer2)."
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

    # Load sequencer2 configs
    print(f"📋 Loading sequencer2 configs: layout={args.layout}")
    merged_services = load_and_merge_configs(workspace=workspace, layout=args.layout)

    main(
        services=merged_services,
        timeout=args.timeout,
        interval=args.interval,
        initial_delay=args.initial_delay,
        namespace=args.namespace,
    )
