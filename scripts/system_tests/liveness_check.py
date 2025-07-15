import argparse
import json
import numbers
import os
import subprocess
import sys
import time
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


def check_service_alive(
    address: str,
    timeout: int,
    interval: int,
    initial_delay: int,
    retry: int = 3,
    retry_delay: int = 1,
) -> bool:
    print("Starting live check test")
    print(f"Initial delay: {initial_delay}s")
    time.sleep(initial_delay)

    start_time = time.time()
    print(f"Start time: {time.strftime('%Y-%m-%d %H:%M:%S', time.localtime(start_time))}")
    print(f"Address: {address}")
    print(f"Timeout: {timeout}s")
    print(f"Interval: {interval}s")
    print(f"Retry: {retry}, Retry delay: {retry_delay}s\n")

    while True:
        elapsed = time.time() - start_time
        if elapsed >= timeout:
            print(f"Successfully ran for {timeout} seconds!")
            return True

        for attempt in range(1, retry + 1):
            try:
                print(f"Calling {address} (attempt {attempt})...")
                response = requests.get(address)
                response.raise_for_status()
                print(response.text)
                break
            except requests.RequestException as e:
                print(f"Attempt {attempt} failed: {e}")
                if attempt == retry:
                    print(f"Failed to call {address} after {retry} attempts.")
                    return False
                time.sleep(retry_delay)

        print(f"Sleeping {interval} seconds before next call.\n")
        time.sleep(interval)


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
    for i, service_name in enumerate(services):
        service_label = f"sequencer-{service_name.lower()}"

        print(f"📱 Finding {service_name} pod...")
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

        print(f"{service_name} pod found - {pod_name}")

        config_paths = get_config_paths(
            deployment_config_path=deployment_config_path,
            service_name=service_name,
        )
        monitoring_port = get_monitoring_endpoint_port(
            base_config_dir=config_dir,
            relative_config_paths=config_paths,
        )

        # Each sequencer is configured to use the same internal port - 8082,
        # so we offset the local port to avoid conflicts when running multiple sequencers locally.
        # This ensures unique local ports per instance.
        local_port = monitoring_port + i
        print(f"🚀 Starting port-forwarding for {service_name} on local port {local_port}...")
        pf_process = subprocess.Popen(
            ["kubectl", "port-forward", pod_name, f"{local_port}:{monitoring_port}"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )

        time.sleep(3)  # Allow port-forward to establish

        try:
            print(f"✅ Running health check for {service_name}...")
            address = f"http://localhost:{local_port}/monitoring/alive"
            success = check_service_alive(
                address=address,
                timeout=timeout,
                interval=interval,
                initial_delay=initial_delay,
            )
            if success:
                print(f"✅ Test passed: {service_name} ran for {timeout} seconds!")
            else:
                print(f"❌ Test failed: {service_name} did not run successfully.")
                pf_process.terminate()
                pf_process.wait()
                sys.exit(1)
        finally:
            pf_process.terminate()
            pf_process.wait()


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
