import argparse
import os
import subprocess
import sys
import time
from typing import Any, Dict, List, Optional, Tuple

from config_loader import find_workspace_root, load_and_merge_configs
from kubernetes import config


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


def make_cmd_verbose_if_needed(cmd: List[str], verbose: bool) -> List[str]:
    """
    Add verbose flag to kubectl command if needed.

    Inserts `-v=6` at position 1 (after "kubectl") when verbose is True.
    """
    if verbose:
        cmd = cmd.copy()  # Don't modify the original list
        cmd.insert(1, "-v=6")
    return cmd


def run(
    cmd: List[str], check: bool = True, capture_output: bool = False
) -> subprocess.CompletedProcess:
    """
    Run a command and handle errors with detailed output.

    When check=True, always capture output to preserve error details on failure.
    """
    # If check=True, we need to capture output to show errors on failure
    should_capture = capture_output or check

    try:
        result = subprocess.run(cmd, check=check, text=True, capture_output=should_capture)
        return result
    except subprocess.CalledProcessError as e:
        # Print detailed error information
        print(f"❌ Command failed: {' '.join(cmd)}")
        if e.stdout:
            print(f"stdout:\n{e.stdout}")
        if e.stderr:
            print(f"stderr:\n{e.stderr}")
        # Re-raise to maintain original behavior
        raise


def copy_state(pod_name: str, namespace: str, data_dir: str, verbose: bool = False) -> None:
    # Clear existing data directory to ensure old database files are removed
    print(f"Clearing existing /data directory in {pod_name}...")
    try:
        exec_cmd = make_cmd_verbose_if_needed(
            [
                "kubectl",
                "exec",
                pod_name,
                "-n",
                namespace,
                "--",
                "sh",
                "-c",
                "rm -rf /data/* 2>/dev/null || true",
            ],
            verbose,
        )
        run(exec_cmd)
        print(f"✅ Cleared /data directory in {pod_name}")
    except subprocess.CalledProcessError as e:
        print(f"⚠️  Warning: Failed to clear /data directory in {pod_name}: {e}")
        print("Continuing with copy operation...")

    print(f"📥 Copying state data to {pod_name}...")
    max_retries = 120
    retry_delay = 0.5  # seconds between retries

    for attempt in range(1, max_retries + 1):
        try:
            cp_cmd = make_cmd_verbose_if_needed(
                [
                    "kubectl",
                    "cp",
                    f"{data_dir}/.",
                    f"{namespace}/{pod_name}:/data",
                ],
                verbose,
            )
            run(cp_cmd)
            print(f"✅ State copied to {pod_name}")
            return  # Success, exit function
        except subprocess.CalledProcessError as e:
            if attempt < max_retries:
                print(
                    f"⚠️  Copy attempt {attempt}/{max_retries} failed, retrying in {retry_delay}s..."
                )
                time.sleep(retry_delay)
            else:
                print(f"❌ Failed to copy state to pod {pod_name} after {max_retries} attempts")
                sys.exit(1)


def delete_pod(pod_name: str, namespace: str, verbose: bool = False) -> None:
    print(f"🔄 Restarting pod {pod_name}...")
    try:
        delete_cmd = make_cmd_verbose_if_needed(
            ["kubectl", "delete", "pod", pod_name, "-n", namespace], verbose
        )
        run(delete_cmd, check=False)
        print(f"✅ Pod {pod_name} restarted successfully!")
    except subprocess.CalledProcessError as e:
        print(f"❌ Failed to delete pod {pod_name}")
        sys.exit(1)


def wait_for_resource(
    controller: str, name: str, namespace: str, timeout: int = 180, verbose: bool = False
) -> None:
    print(f"⏳ Waiting for {controller}/{name} to become ready...")

    if controller == "deployment":
        cmd = [
            "kubectl",
            "wait",
            "--for=condition=Available",
            f"{controller}/{name}",
            "-n",
            namespace,
            f"--timeout={timeout}s",
        ]
    elif controller == "statefulset":
        cmd = [
            "kubectl",
            "rollout",
            "status",
            f"{controller}/{name}",
            "-n",
            namespace,
            f"--timeout={timeout}s",
        ]
    else:
        print(f"❌ Unknown controller type: {controller}. Aborting...")
        sys.exit(1)

    cmd = make_cmd_verbose_if_needed(cmd, verbose)

    try:
        run(cmd)
    except subprocess.CalledProcessError:
        print(f"⚠️ Timeout waiting for {controller.capitalize()} {name}")
        sys.exit(1)


# TODO(Nadin): Move this function to utils and use it across all the scripts.
def build_resource_name(service_name: str, controller: str) -> str:
    return f"sequencer-{service_name.lower()}-{controller.lower()}"


def main(
    layout: str, namespace: str, data_dir: str, overlay: Optional[str] = None, verbose: bool = False
) -> None:
    config.load_kube_config()

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
            get_cmd = make_cmd_verbose_if_needed(
                [
                    "kubectl",
                    "get",
                    "pods",
                    "-n",
                    namespace,
                    "-l",
                    f"service={service_label}",
                    "-o",
                    "jsonpath={.items[0].metadata.name}",
                ],
                verbose,
            )
            pod_name = run(get_cmd, capture_output=True).stdout.strip()
        except subprocess.CalledProcessError:
            print(f"❌ Missing pod for {service_name}. Aborting!")
            sys.exit(1)

        if not pod_name:
            print(f"❌ No pod found for {service_name}. Aborting!")
            sys.exit(1)

        print(f"{service_name} pod found - {pod_name}")

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
    run(["kubectl", "get", "pods", "-n", namespace, "-o", "wide"])

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
