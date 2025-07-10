import os

prometheus_service_name = "prometheus-service"
network_stress_test_deployment_file_name = "network_stress_test_deployment_file.json"
# got this by running: cargo run --bin get_peer_id_from_secret_key 0x0000000000000000000000000000000000000000000000000000000000000000
bootstrap_peer_id = "12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN"
project_root = os.path.abspath("../../../../../../")
assert project_root.endswith("/sequencer"), "Project root must end in '/sequencer'"


def run_cmd(cmd: str, hint: str = "none", may_fail: bool = False):
    print(f"🔧 CMD: {cmd}", flush=True)
    result = os.system(cmd)
    if result != 0 and not may_fail:
        raise RuntimeError(
            f"⚠️ Command failed with exit code {result}: {cmd}\nHint: {hint}"
        )


def pr(string: str):
    print(f"🔔 INFO: {string}", flush=True)


def connect_to_cluster():
    run_cmd(
        "gcloud container clusters get-credentials sequencer-dev --region us-central1 --project starkware-dev",
        hint="Make sure you have gcloud installed and you are logged in.",
    )
