"""
Populate PVCs with state data before deploying sequencer workloads.

Uses config_loader to detect all services with persistentVolume.enabled,
creates a temporary Job per service that mounts the PVC, copies local
state data into the Job pod, then deletes the Job so the PVC is ready
for the StatefulSet/Deployment.
"""
import argparse
import os
import subprocess
import sys
import time
from typing import Any, Dict, List, Optional, Tuple

from config_loader import find_workspace_root, load_and_merge_configs
from kubernetes import client, config
from kubernetes.client.rest import ApiException


def run(
    cmd: List[str], check: bool = True, capture_output: bool = False
) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, check=check, text=True, capture_output=capture_output)


def get_services_with_pvc(
    merged_services: List[Dict[str, Any]],
) -> List[Tuple[str, str]]:
    """
    Return list of (service_name, pvc_name) for services with persistentVolume.enabled.
    PVC name follows deployment convention: sequencer-{service_name}-data
    """
    result: List[Tuple[str, str]] = []
    for service_config in merged_services:
        pv_config = service_config.get("persistentVolume") or {}
        if not pv_config.get("enabled", False):
            continue
        service_name = service_config.get("name")
        if not service_name:
            continue
        pvc_name = f"sequencer-{service_name.lower()}-data"
        result.append((service_name, pvc_name))
    return result


def create_job(
    batch_v1: client.BatchV1Api,
    namespace: str,
    job_name: str,
    pvc_name: str,
    sleep_seconds: int = 600,
) -> None:
    """Create a Job that mounts the PVC and runs sleep to keep the pod alive."""
    volume = client.V1Volume(
        name="data",
        persistent_volume_claim=client.V1PersistentVolumeClaimVolumeSource(claim_name=pvc_name),
    )
    volume_mount = client.V1VolumeMount(name="data", mount_path="/data")
    container = client.V1Container(
        name="populate",
        image="busybox:1.36",
        command=["sh", "-c", f"sleep {sleep_seconds}"],
        volume_mounts=[volume_mount],
    )
    template = client.V1PodTemplateSpec(
        metadata=client.V1ObjectMeta(labels={"app": "populate-pvc", "job": job_name}),
        spec=client.V1PodSpec(
            restart_policy="Never",
            containers=[container],
            volumes=[volume],
        ),
    )
    job_spec = client.V1JobSpec(template=template, backoff_limit=0, ttl_seconds_after_finished=60)
    job = client.V1Job(
        api_version="batch/v1",
        kind="Job",
        metadata=client.V1ObjectMeta(name=job_name, namespace=namespace),
        spec=job_spec,
    )
    batch_v1.create_namespaced_job(namespace=namespace, body=job)


def wait_for_job_pod(
    core_v1: client.CoreV1Api,
    namespace: str,
    job_name: str,
    timeout_seconds: int = 120,
    poll_interval: float = 2.0,
) -> str:
    """Wait for the Job pod to be running and return its name."""
    start = time.monotonic()
    while (time.monotonic() - start) < timeout_seconds:
        pods = core_v1.list_namespaced_pod(
            namespace=namespace,
            label_selector=f"job-name={job_name}",
        )
        if pods.items:
            pod = pods.items[0]
            if pod.status.phase == "Running":
                return pod.metadata.name
        time.sleep(poll_interval)
    raise TimeoutError(f"Job pod for {job_name} did not become Running within {timeout_seconds}s")


def copy_data_to_pod(pod_name: str, namespace: str, data_dir: str) -> None:
    """Copy local data_dir into the pod's /data directory."""
    if not os.path.isdir(data_dir):
        raise FileNotFoundError(f"Data directory not found: {data_dir}")
    run(
        [
            "kubectl",
            "cp",
            f"{data_dir}/.",
            f"{namespace}/{pod_name}:/data",
            "--retries=3",
        ],
    )


def delete_job(batch_v1: client.BatchV1Api, namespace: str, job_name: str) -> None:
    """Delete the Job and optionally wait for it to be gone."""
    try:
        batch_v1.delete_namespaced_job(
            name=job_name,
            namespace=namespace,
            propagation_policy="Background",
        )
    except ApiException as e:
        if e.status != 404:
            raise


def main(
    layout: str,
    namespace: str,
    data_dir: str,
    overlay: Optional[str] = None,
) -> None:
    config.load_kube_config()

    workspace = os.environ.get("GITHUB_WORKSPACE")
    if not workspace:
        workspace = find_workspace_root()
        if workspace:
            print(f"📁 Auto-detected workspace: {workspace}")

    if not workspace:
        print("❌ Could not determine workspace root.")
        print("   Set GITHUB_WORKSPACE env var or ensure script is in scripts/system_tests/")
        sys.exit(1)

    overlay_info = f", overlay={overlay}" if overlay else ""
    print(f"📋 Loading sequencer configs: layout={layout}{overlay_info}")
    merged_services = load_and_merge_configs(workspace=workspace, layout=layout, overlay=overlay)

    services_with_pvc = get_services_with_pvc(merged_services)
    if not services_with_pvc:
        print("No services with PVCs found. Skipping PVC population.")
        return

    print(f"📦 Found {len(services_with_pvc)} service(s) with PVCs to populate")
    batch_v1 = client.BatchV1Api()
    core_v1 = client.CoreV1Api()

    for service_name, pvc_name in services_with_pvc:
        job_name = f"populate-pvc-{service_name.lower()}"
        print(f"\n🚀 Populating PVC for service: {service_name} (PVC: {pvc_name})")
        try:
            create_job(batch_v1, namespace, job_name, pvc_name)
            pod_name = wait_for_job_pod(core_v1, namespace, job_name)
            print(f"   Pod ready: {pod_name}")
            copy_data_to_pod(pod_name, namespace, data_dir)
            print(f"   ✅ Data copied to PVC")
        except Exception as e:
            print(f"   ❌ Failed: {e}")
            raise
        finally:
            delete_job(batch_v1, namespace, job_name)
            print(f"   Job {job_name} deleted")

    print("\n✅ All PVCs populated successfully.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Populate PVCs with state data before deploying sequencer (per-service, auto-detected)."
    )
    parser.add_argument("--layout", type=str, required=True, help="Layout name (e.g., 'hybrid')")
    parser.add_argument("--namespace", type=str, required=True, help="Kubernetes namespace")
    parser.add_argument(
        "--data-dir",
        type=str,
        default="./output/data/node_0",
        help="Directory containing the state to copy into PVCs (default: ./output/data/node_0)",
    )
    parser.add_argument(
        "--overlay",
        type=str,
        default=None,
        help="Overlay path in dot notation (e.g., 'hybrid.testing.node-0')",
    )
    args = parser.parse_args()
    main(
        layout=args.layout,
        namespace=args.namespace,
        data_dir=args.data_dir,
        overlay=args.overlay,
    )
