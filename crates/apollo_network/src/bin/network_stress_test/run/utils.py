import os
import time

prometheus_service_name = "prometheus-service"
network_stress_test_deployment_file_name = "network_stress_test_deployment_file.json"
# got this by running: cargo run --bin get_peer_id_from_secret_key 0x0000000000000000000000000000000000000000000000000000000000000000
bootstrap_peer_id = "12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN"
project_root = os.path.abspath("../../../../../../")
assert project_root.endswith("/sequencer"), "Project root must end in '/sequencer'"


def run_cmd(cmd: str, hint: str = "none", may_fail: bool = False):
    print(f"🔧🔧🔧 CMD: {cmd}", flush=True)
    result = os.system(cmd)
    if result != 0 and not may_fail:
        raise RuntimeError(
            f"Command failed with exit code {result}: {cmd}\n ⚠️ ⚠️ ⚠️  Hint: {hint}"
        )


def pr(string: str):
    print(f"🔔 INFO: {string}", flush=True)


def check_docker():
    pr("Checking if Docker works...")
    run_cmd(
        "docker run hello-world",
        hint="Make sure you have Docker installed and running.",
    )
    pr("Docker is working correctly.")


def connect_to_cluster():
    run_cmd(
        "gcloud container clusters get-credentials sequencer-dev --region us-central1 --project starkware-dev",
        hint="Make sure you have gcloud installed and you are logged in (run `gcloud auth login`).",
    )


def make_time_stamp() -> str:
    return time.strftime("%Y-%m-%d-%H-%M-%S", time.localtime())


def get_prometheus_config(self_scrape: bool, metric_urls: list[str]) -> str:
    result = "global:\n"
    result += "  scrape_interval: 1s\n"
    result += "scrape_configs:\n"
    if self_scrape:
        result += f"  - job_name: prometheus\n"
        result += f"    static_configs:\n"
        result += f"      - targets: ['localhost:9090']\n"
    for i, url in enumerate(metric_urls):
        result += f"  - job_name: 'network_stress_test_{i}'\n"
        result += f"    static_configs:\n"
        result += f"      - targets: ['{url}']\n"
        result += f"        labels:\n"
        result += f"          application: 'network_stress_test'\n"
        result += f"          environment: 'test'\n"
    return result
