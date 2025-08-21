import argparse
import json
import numbers
import os
import subprocess
import sys
import time
import socket
import requests
from typing import List, Union
from multiprocessing import Process, Queue


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
    i: int,
    config_dir: str,
    timeout: int,
    interval: int,
    initial_delay: int,
    q: Queue,
):
    try:
        monitoring_port = get_monitoring_endpoint_port(
            base_config_dir=config_dir,
            relative_config_paths=config_paths,
        )
        local_port = monitoring_port + i
        print(f"[{service_name}] 🚀 Port-forwarding on {local_port} -> {monitoring_port}")

        pf_process = subprocess.Popen(
            ["kubectl", "port-forward", pod_name, f"{local_port}:{monitoring_port}"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )

        try:
            if not wait_for_port("127.0.0.1", local_port, timeout=15):
                q.put((service_name, False, "Port-forward did not establish"))
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
                q.put((service_name, True, "Health check passed"))
            else:
                print(f"[{service_name}] ❌ Failed health check")
                q.put((service_name, False, "Health check failed"))
        finally:
            pf_process.terminate()
            pf_process.wait()
    except Exception as e:
        q.put((service_name, False, str(e)))


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
    services = get_services(deployment_config_path=deployment_config_path)
    print("📱 Finding pods for services...")

    q = Queue()
    procs: List[Process] = []

    for i, service_name in enumerate(services):
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

        p = Process(
            target=run_service_check,
            args=(
                service_name,
                pod_name,
                config_paths,
                i,
                config_dir,
                timeout,
                interval,
                initial_delay,
                q,
            ),
        )
        p.start()
        procs.append(p)

    # join with timeout guard
    results = []
    for p in procs:
        p.join(timeout=timeout + initial_delay + 30)
        if p.is_alive():
            print(f"⚠️ Killing hung process {p.pid}")
            p.terminate()
            p.join()

    while not q.empty():
        results.append(q.get())

    print("\n=== RESULTS ===")
    all_ok = True
    for svc, ok, msg in results:
        status = "✅" if ok else "❌"
        print(f"{status} {svc} - {msg}")
        if not ok:
            all_ok = False

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
