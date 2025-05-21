import argparse
import json
import os
import subprocess
import sys
import time
from typing import List


def run(
    cmd: List[str], capture_output=False, check=True, text=True
) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=capture_output, check=check, text=text)


def get_services(deployment_config_path: str) -> List[str]:
    with open(deployment_config_path, "r", encoding="utf-8") as f:
        config = json.load(f)
    return [s["name"] for s in config.get("services", [])]


def get_config_path(deployment_config_path: str, service_name: str) -> str:
    with open(deployment_config_path, "r", encoding="utf-8") as f:
        config = json.load(f)
    for service in config["services"]:
        if service["name"] == service_name:
            paths = service.get("config_paths", [])
            if not paths:
                raise ValueError(f"No config_paths found for service {service_name}")
            return paths[0]
    raise ValueError(f"Service {service_name} not found in deployment config")


def get_monitoring_port(config_file_path: str) -> int:
    with open(config_file_path, "r", encoding="utf-8") as f:
        config = json.load(f)
    return config["monitoring_endpoint_config.port"]


def main(
    deployment_config_path: str,
    config_dir: str,
    timeout: int,
    interval: int,
    initial_delay: int,
):
    print(
        f"Running liveness checks on config_dir: {config_dir} and deployment_config_path: {deployment_config_path} "
    )
    services = get_services(deployment_config_path)
    print("📡 Finding pods for services...")
    for i, service_name in enumerate(services):
        service_label = f"sequencer-{service_name.lower()}"

        print(f"📡 Finding {service_name} pod...")
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

        config_path = get_config_path(deployment_config_path, service_name)
        full_config_path = os.path.join(config_dir, config_path)
        monitoring_port = get_monitoring_port(full_config_path)

        local_port = monitoring_port + i
        print(
            f"🚀 Starting port-forwarding for {service_name} on local port {local_port}..."
        )
        pf_process = subprocess.Popen(
            ["kubectl", "port-forward", pod_name, f"{local_port}:{monitoring_port}"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )

        time.sleep(3)  # Allow port-forward to establish

        try:
            print(f"✅ Running health check for {service_name}...")
            result = subprocess.run(
                [
                    "./devops/scripts/check_alive.sh",
                    "--address",
                    f"http://localhost:{local_port}/monitoring/alive",
                    "--timeout",
                    str(timeout),
                    "--interval",
                    str(interval),
                    "--initial-delay",
                    str(initial_delay),
                ],
                check=False,
            )
            if result.returncode == 0:
                print(f"✅ Test passed: {service_name} ran for {timeout} seconds!")
            else:
                print(f"❌ Test failed: {service_name} did not run successfully.")
                pf_process.terminate()
                pf_process.wait()
                sys.exit(result.returncode)
        finally:
            pf_process.terminate()
            pf_process.wait()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Run liveness checks on Kubernetes services."
    )
    parser.add_argument("deployment_config_path", help="Path to the deployment config JSON file")
    parser.add_argument("config_dir", help="Base directory for service config files")
    parser.add_argument("timeout", type=int, help="Timeout duration in seconds for each service check")
    parser.add_argument("interval", type=int, help="Interval between health checks in seconds")
    parser.add_argument(
        "--initial-delay",
        type=int,
        default=int(os.getenv("INITIAL_DELAY_SEC", "10")),
        help="Initial delay before starting health checks (default: value from INITIAL_DELAY_SEC env var or 10)",
    )

    args = parser.parse_args()

    main(
        deployment_config_path=args.deployment_config_path,
        config_dir=args.config_dir,
        timeout=args.timeout,
        interval=args.interval,
        initial_delay=args.initial_delay,
    )
