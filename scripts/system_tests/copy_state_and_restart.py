import json
import subprocess
import sys
from typing import List, Tuple

from kubernetes import config


def run(
    cmd: List[str], check: bool = True, capture_output: bool = False
) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, check=check, text=True, capture_output=capture_output)


def load_services(deployment_config_path: str) -> List[Tuple[str, str]]:
    with open(deployment_config_path, "r", encoding="utf-8") as f:
        deployment_config = json.load(f)
    return [
        (svc["name"], svc["controller"])
        for svc in deployment_config.get("services", [])
    ]


def copy_state(pod_name: str, data_dir: str) -> None:
    print(f"📥 Copying state data to {pod_name}...")
    try:
        run(
            [
                "kubectl",
                "cp",
                f"{data_dir}/.",
                f"{pod_name}:/data",
                "--retries=3",
            ]
        )
    except subprocess.CalledProcessError as e:
        print(f"❌ Failed to copy state to pod {pod_name}: {e}")
        sys.exit(1)


def delete_pod(pod_name: str) -> None:
    print(f"🔄 Restarting pod {pod_name}...")
    run(["kubectl", "delete", "pod", pod_name], check=False)


def wait_for_resource(controller: str, name: str, timeout: int = 180) -> None:
    print(f"⏳ Waiting for {controller}/{name} to become ready...")

    if controller == "deployment":
        cmd = [
            "kubectl",
            "wait",
            "--for=condition=Available",
            f"{controller}/{name}",
            f"--timeout={timeout}s",
        ]
    elif controller == "statefulset":
        cmd = [
            "kubectl",
            "rollout",
            "status",
            f"{controller}/{name}",
            f"--timeout={timeout}s",
        ]
    else:
        print(f"❌ Unknown controller type: {controller}. Aborting...")
        sys.exit(1)

    try:
        run(cmd)
    except subprocess.CalledProcessError:
        print(f"⚠️ Timeout waiting for {controller.capitalize()} {name}")
        sys.exit(1)


# TODO(Nadin): Move this function to utils and use it across all the scripts.
def build_resource_name(service_name: str, controller: str) -> str:
    return f"sequencer-{service_name.lower()}-{controller.lower()}"


def main(deployment_config_path: str, data_dir: str) -> None:

    config.load_kube_config()
    services: List[Tuple[str, str]] = load_services(deployment_config_path)

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

        copy_state(pod_name=pod_name, data_dir=data_dir)
        delete_pod(pod_name=pod_name)

        resources_to_wait_for.append((controller_lower, resource_name))

    print("\n⏳ Waiting for all resources to become ready...\n")
    for controller, resource_name in resources_to_wait_for:
        wait_for_resource(controller=controller, name=resource_name)
        print(f"✅ {controller}/{resource_name} is ready!")

    print("\n📦 Current pod status:")
    run(["kubectl", "get", "pods", "-o", "wide"])

    print("\n✅ All services are ready!")


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(
        description="Copy state and restart sequencer pods based on a deployment config."
    )

    parser.add_argument(
        "--deployment_config_path",
        type=str,
        required=True,
        help="Path to the deployment config JSON file",
    )

    parser.add_argument(
        "--data-dir",
        type=str,
        default="./output/data/node_0",
        help="Directory containing the state to copy into pods (default: ./output/data/node_0)",
    )

    args = parser.parse_args()
    main(deployment_config_path=args.deployment_config_path, data_dir=args.data_dir)
