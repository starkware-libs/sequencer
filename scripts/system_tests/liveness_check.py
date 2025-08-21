import argparse
import json
import numbers
import os
import socket
import subprocess
import sys
import time
from multiprocessing import Process, Queue
from typing import List, Union

import requests


def run(
    cmd: List[str], capture_output: bool = False, check: bool = True, text: bool = True
) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=capture_output, check=check, text=text)


def get_services(deployment_config_path: str) -> List[str]:
    with open(deployment_config_path, "r", encoding="utf-8") as f:
        config = json.load(f)
    return [s["name"] for s in config.get("services", [])]


def get_config_paths(deployment_config_path: str, service_name: str) -> List[str]:
    with open(deployment_config_path, "r", encoding="utf-8") as f:
        config = json.load(f)
    for service in config["services"]:
        if service["name"] == service_name:
            return service.get("config_paths", [])
    raise ValueError(f"Service {service_name} not found in deployment config")


def get_monitoring_endpoint_port(
    base_config_dir: str, relative_config_paths: List[str]
) -> Union[int, float]:
    for relativ_config_path in relative_config_paths:
        path = os.path.join(base_config_dir, relativ_config_path)
        try:
            with open(path, "r", encoding="utf-8") as f:
                data = json.load(f)
                value = data.get("monitoring_endpoint_config.port")

                if isinstance(value, numbers.Number):
                    return value

        except (json.JSONDecodeError, FileNotFoundError) as e:
            print(f"Warning: Skipping {path} due to error: {e}")
            continue
    raise ValueError(
        "monitoring_endpoint_config.port not found or not a valid number in any config file"
    )


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
    config_paths: List[str],
    offset: int,
    config_dir: str,
    timeout: int,
    interval: int,
    initial_delay: int,
    process_queue: Queue,
):
    try:
        monitoring_port = get_monitoring_endpoint_port(
            base_config_dir=config_dir,
            relative_config_paths=config_paths,
        )
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
    deployment_config_path: str,
    config_dir: str,
    timeout: int,
    interval: int,
    initial_delay: int,
):
    print(
        f"Running liveness checks on config_dir: {config_dir} and deployment_config_path: {deployment_config_path}"
    )
    print(f"Timeout: {timeout}s")
    print(f"Interval: {interval}s")
    print(f"Initial Delay: {initial_delay}s")

    print("📱 Finding pods for services...")
    services = get_services(deployment_config_path=deployment_config_path)

    process_queue = Queue()
    healthcheck_processes: List[Process] = []

    for offset, service_name in enumerate(services):
        service_label = f"sequencer-{service_name.lower()}"
        try:
            pod_name = run(
                [
                    "kubectl",
                    "get",
                    "pods",
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

        config_paths = get_config_paths(
            deployment_config_path=deployment_config_path,
            service_name=service_name,
        )

        process = Process(
            name=service_name,
            target=run_service_check,
            args=(
                service_name,
                pod_name,
                config_paths,
                offset,
                config_dir,
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
    parser = argparse.ArgumentParser(description="Run liveness checks on Kubernetes services.")
    parser.add_argument(
        "--deployment-config-path",
        type=str,
        required=True,
        help="Path to the deployment config JSON file",
    )
    parser.add_argument(
        "--config-dir",
        type=str,
        required=True,
        help="Base directory for service config files",
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

    main(
        deployment_config_path=args.deployment_config_path,
        config_dir=args.config_dir,
        timeout=args.timeout,
        interval=args.interval,
        initial_delay=args.initial_delay,
    )
