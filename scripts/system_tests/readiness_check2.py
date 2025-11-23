import argparse
import json
import os
import sys
import tempfile
import time
from copy import deepcopy
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

import yaml
from kubernetes import client, config
from kubernetes.client.rest import ApiException


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

    Script is at: scripts/system_tests/readiness_check2.py
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


def extract_service_info_from_config(service_config: Dict[str, Any]) -> Tuple[str, str, str]:
    """
    Extract service info from merged service config.

    Returns:
        (controller, service_name_lower, controller_lower)
    """
    service_name = service_config.get("name", "")

    # Determine controller type from statefulSet config
    stateful_set = service_config.get("statefulSet", {})
    if stateful_set.get("enabled", False):
        controller = "StatefulSet"
    else:
        controller = "Deployment"

    return controller, service_name.lower(), controller.lower()


def convert_to_legacy_format(services: List[Dict[str, Any]]) -> Dict[str, Any]:
    """
    Convert sequencer2 service configs to legacy JSON format.

    This allows the rest of the script to work without changes.
    """
    legacy_services = []

    for service_config in services:
        controller, service_name_lower, controller_lower = extract_service_info_from_config(
            service_config
        )

        service_entry = {
            "name": service_config["name"],
            "controller": controller,
        }

        legacy_services.append(service_entry)

    return {
        "application_config_subdir": "crates/apollo_deployments/resources/",  # Default, not used
        "services": legacy_services,
    }


def check_manifest_files(deployment_config_path: str, workspace: str, namespace: str) -> None:
    """
    Check that manifest files exist.

    For sequencer2, the path structure is:
    deployments/sequencer2/dist/sequencer-{service_name}/{Controller}.sequencer-{service_name}-{controller}.k8s.yaml
    """
    with open(deployment_config_path, "r", encoding="utf-8") as f:
        deployment_config: Dict[str, Any] = json.load(f)

    services = deployment_config["services"]

    for service in services:
        controller, service_name_lower, controller_lower = extract_service_info(service)

        manifest_path = (
            Path(workspace)
            / "deployments"
            / "sequencer2"
            / "dist"
            / f"sequencer-{service_name_lower}"
            / f"{controller}.sequencer-{service_name_lower}-{controller_lower}.k8s.yaml"
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
    """Extract service metadata from config."""
    service_name = service["name"]
    controller = service["controller"]
    return controller, service_name.lower(), controller.lower()


def wait_for_services_ready(deployment_config_path: str, namespace: str) -> None:
    """Wait for Kubernetes resources to become ready."""
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
                obj = apps_v1.read_namespaced_stateful_set(name=resource_name, namespace=namespace)
            elif controller_lower == "deployment":
                obj = apps_v1.read_namespaced_deployment(name=resource_name, namespace=namespace)
            else:
                print(f"❌ Unknown controller: {controller}. Skipping...")
                sys.exit(1)
        except ApiException as e:
            print(f"❌ API Exception occurred: {e}")
            raise

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
                    status = apps_v1.read_namespaced_stateful_set_status(
                        name=resource_name, namespace=namespace
                    ).status
                    ready = status.ready_replicas or 0
                    desired = status.replicas or 0
                elif controller_lower == "deployment":
                    status = apps_v1.read_namespaced_deployment_status(
                        name=resource_name, namespace=namespace
                    ).status
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
        description="Check manifest files and wait for K8s services to be ready (sequencer2)."
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

    # Load sequencer2 configs and convert to legacy format
    print(f"📋 Loading sequencer2 configs: layout={args.layout}")
    merged_services = load_and_merge_configs(workspace=workspace, layout=args.layout)

    # Convert to legacy format for compatibility
    legacy_config = convert_to_legacy_format(merged_services)
    legacy_config["namespace"] = args.namespace

    # Write to temp file for the rest of the script to use
    with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
        json.dump(legacy_config, f, indent=2)
        temp_config_path = f.name

    try:
        check_manifest_files(
            deployment_config_path=temp_config_path,
            workspace=workspace,
            namespace=args.namespace,
        )
        wait_for_services_ready(deployment_config_path=temp_config_path, namespace=args.namespace)
        print("✅ All sequencer services are ready.")
    finally:
        # Clean up temp file
        if os.path.exists(temp_config_path):
            os.unlink(temp_config_path)
