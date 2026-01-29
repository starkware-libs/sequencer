import argparse
import os
import sys
from typing import Any, Dict, List, Optional, Tuple

from utils.config_loader import find_workspace_root, load_and_merge_configs
from utils.k8s_utils import (
    copy_to_pod,
    delete_pod,
    exec_in_pod,
    get_pod_name,
    wait_for_resource,
)


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


def copy_state(pod_name: str, namespace: str, data_dir: str, verbose: bool = False) -> None:
    # Clear existing data directory to ensure old database files are removed
    print(f"Clearing existing /data directory in {pod_name}...")
    try:
        exec_in_pod(
            pod_name=pod_name,
            namespace=namespace,
            command=["sh", "-c", "rm -rf /data/* 2>/dev/null || true"],
            verbose=verbose,
        )
        print(f"✅ Cleared /data directory in {pod_name}")
    except RuntimeError as e:
        print(f"⚠️  Warning: Failed to clear /data directory in {pod_name}: {e}")
        print("Continuing with copy operation...")

    print(f"📥 Copying state data to {pod_name}...")
    try:
        copy_to_pod(
            pod_name=pod_name,
            namespace=namespace,
            local_path=f"{data_dir}/.",
            remote_path="/data",
            verbose=verbose,
        )
        print(f"✅ State copied to {pod_name}")
    except RuntimeError as e:
        print(f"❌ Failed to copy state to pod {pod_name}: {e}")
        sys.exit(1)


# TODO(Nadin): Move this function to utils and use it across all the scripts.
def build_resource_name(service_name: str, controller: str) -> str:
    return f"sequencer-{service_name.lower()}-{controller.lower()}"


def main(
    layout: str, namespace: str, data_dir: str, overlay: Optional[str] = None, verbose: bool = False
) -> None:
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

    # Load sequencer configs
    overlay_info = f", overlay={overlay}" if overlay else ""
    print(f"📋 Loading sequencer configs: layout={layout}{overlay_info}")
    merged_services = load_and_merge_configs(workspace=workspace, layout=layout, overlay=overlay)

    # Extract service info from merged configs
    services: List[Tuple[str, str]] = []
    for service_config in merged_services:
        controller, service_name_lower, controller_lower = extract_service_info_from_config(
            service_config
        )
        services.append((service_config["name"], controller))

    resources_to_wait_for: List[Tuple[str, str]] = []

    # Reverse the service order so the batcher (which must restart last in the distributed flow) is
    #  handled after the others.
    # TODO(Nadin): Investigate why a specific restart order is needed and whether it can be enforced
    #  explicitly instead of relying on reversed().
    for service_name, controller in reversed(services):
        service_name_lower = service_name.lower()
        controller_lower = controller.lower()
        resource_name = build_resource_name(service_name, controller)

        print(f"🚀 Processing service: {service_name} ({controller})")

        service_label = f"sequencer-{service_name_lower}"

        print(f"📡 Finding {service_name} pod...")
        try:
            pod_name = get_pod_name(
                label_selector=f"service={service_label}",
                namespace=namespace,
                verbose=verbose,
            )
            print(f"{service_name} pod found - {pod_name}")
        except RuntimeError as e:
            print(f"❌ Missing pod for {service_name}: {e}")
            sys.exit(1)

        copy_state(pod_name=pod_name, namespace=namespace, data_dir=data_dir, verbose=verbose)
        delete_pod(pod_name=pod_name, namespace=namespace, verbose=verbose)

        resources_to_wait_for.append((controller_lower, resource_name))

    print("\n⏳ Waiting for all resources to become ready...\n")
    for controller, resource_name in resources_to_wait_for:
        wait_for_resource(
            controller=controller, name=resource_name, namespace=namespace, verbose=verbose
        )
        print(f"✅ {controller}/{resource_name} is ready!")

    print(f"\n📦 Current pod status in namespace {namespace}:")
    # Simple status display - can be enhanced later if needed
    print(f"(Use 'kubectl get pods -n {namespace} -o wide' for detailed status)")

    print("\n✅ All services are ready!")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Copy state and restart sequencer pods based on sequencer layout config."
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
        "--data-dir",
        type=str,
        default="./output/data/node_0",
        help="Directory containing the state to copy into pods (default: ./output/data/node_0)",
    )
    parser.add_argument(
        "--overlay",
        type=str,
        default=None,
        help="Overlay path in dot notation (e.g., 'hybrid.testing.node-0')",
    )
    parser.add_argument(
        "--verbose",
        action="store_true",
        help="Enable verbose kubectl output (adds -v=6 flag to kubectl commands)",
    )

    args = parser.parse_args()
    main(
        layout=args.layout,
        namespace=args.namespace,
        data_dir=args.data_dir,
        overlay=args.overlay,
        verbose=args.verbose,
    )
