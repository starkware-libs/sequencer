import argparse
import json
import os
import sys
import time
from pathlib import Path
from typing import Any, Dict, Tuple, Union, cast

from kubernetes import client, config
from kubernetes.client.rest import ApiException


def check_manifest_files(deployment_config_path: str, workspace: str) -> None:
    with open(deployment_config_path, "r", encoding="utf-8") as f:
        deployment_config: Dict[str, Any] = json.load(f)

    services = deployment_config["services"]

    for service in services:
        controller, service_name_lower, controller_lower = extract_service_info(service)

        manifest_path = (
            Path(workspace)
            / f"deployments/sequencer/dist/sequencer-{service_name_lower}/{controller}.sequencer-{service_name_lower}-{controller_lower}.k8s.yaml"
        )

        if not manifest_path.exists():
            print(f"❌ Manifest {manifest_path} for {service_name_lower} not found. Aborting...")
            try:
                dir_listing = list(manifest_path.parent.iterdir())
                print(f"Contents of {manifest_path.parent}: {[str(f) for f in dir_listing]}")
            except FileNotFoundError:
                print(f"(Directory {manifest_path.parent} does not exist)")
            sys.exit(1)


def extract_service_info(service: Dict[str, str]) -> Tuple[str, str, str]:
    service_name = service["name"]
    controller = service["controller"]
    return controller, service_name.lower(), controller.lower()


def wait_for_services_ready(deployment_config_path: str, namespace: str) -> None:
    config.load_kube_config()

    with open(deployment_config_path, "r", encoding="utf-8") as f:
        deployment_config: Dict[str, Any] = json.load(f)

    services = deployment_config["services"]

    apps_v1 = client.AppsV1Api()

    for service in services:
        controller, service_name_lower, controller_lower = extract_service_info(service)

        resource_name = f"sequencer-{service_name_lower}-{controller_lower}"

        print(f"🔍 Checking {controller_lower}: {resource_name}")

        try:
            if controller_lower == "statefulset":
                obj: Union[client.V1Deployment, client.V1StatefulSet] = cast(
                    client.V1StatefulSet,
                    apps_v1.read_namespaced_stateful_set(  # type: ignore[no-untyped-call]
                        name=resource_name, namespace=namespace
                    ),
                )
            elif controller_lower == "deployment":
                obj = cast(
                    client.V1Deployment,
                    apps_v1.read_namespaced_deployment(  # type: ignore[no-untyped-call]
                        name=resource_name, namespace=namespace
                    ),
                )
            else:
                print(f"❌ Unknown controller: {controller}. Skipping...")
                sys.exit(1)
        except ApiException as e:
            print(f"❌ API Exception occurred: {e}")
            raise

        assert isinstance(obj.status, (client.V1DeploymentStatus, client.V1StatefulSetStatus))
        print(
            f"🔍 {controller} {resource_name} status: replicas={obj.status.replicas}, ready={obj.status.ready_replicas}"
        )

        print(f"⏳ Waiting for {controller_lower}/{resource_name} to become ready...")

        timeout_seconds = 180
        poll_interval = 5
        elapsed = 0

        while elapsed < timeout_seconds:
            try:
                if controller_lower == "statefulset":
                    status = cast(
                        client.V1StatefulSet,
                        apps_v1.read_namespaced_stateful_set_status(  # type: ignore[no-untyped-call]
                            name=resource_name, namespace=namespace
                        ),
                    ).status
                    assert isinstance(status, client.V1StatefulSetStatus)
                    ready = status.ready_replicas or 0
                    desired = status.replicas or 0
                elif controller_lower == "deployment":
                    status = cast(
                        client.V1Deployment,
                        apps_v1.read_namespaced_deployment_status(  # type: ignore[no-untyped-call]
                            name=resource_name, namespace=namespace
                        ),
                    ).status
                    assert isinstance(status, client.V1DeploymentStatus)
                    ready = status.ready_replicas or 0
                    desired = status.replicas or 0
                else:
                    print(f"❌ Unknown controller: {controller}.")
                    sys.exit(1)

                if ready == desired and ready > 0:
                    print(f"✅ {controller} {resource_name} is ready.")
                    break
            except ApiException as e:
                print(f"❌ Error while checking status: {e}")

            time.sleep(poll_interval)
            elapsed += poll_interval
        else:
            print(f"❌ Timeout waiting for {controller} {resource_name} to become ready.")
            sys.exit(1)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Check manifest files and wait for K8s services to be ready."
    )
    parser.add_argument(
        "--deployment_config_path",
        type=str,
        required=True,
        help="Path to the deployment config JSON file",
    )
    parser.add_argument(
        "--namespace",
        type=str,
        required=True,
        help="Kubernetes namespace",
    )
    args = parser.parse_args()

    github_workspace: str = os.environ["GITHUB_WORKSPACE"]

    check_manifest_files(
        deployment_config_path=args.deployment_config_path,
        workspace=github_workspace,
    )

    wait_for_services_ready(
        deployment_config_path=args.deployment_config_path,
        namespace=args.namespace,
    )

    print("✅ All sequencer services are ready.")
