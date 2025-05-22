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
        config = json.load(f)
    return [(svc["name"], svc["controller"]) for svc in config.get("services", [])]


def copy_state(pod_name: str, data_dir: str):
    print(f"📥 Copying state data to {pod_name}...")
    try:
        run(
            [
                "kubectl",
                "cp",
                data_dir,
                f"{pod_name}:/data",
                "--retries=3",
            ]
        )
    except subprocess.CalledProcessError as e:
        print(f"❌ Failed to copy state to pod {pod_name}: {e}")
        sys.exit(1)


def delete_pod(pod_name: str):
    print(f"🔄 Restarting pod {pod_name}...")
    run(["kubectl", "delete", "pod", pod_name], check=False)


def wait_for_resource(controller: str, name: str, timeout: int = 180):
    print(f"⏳ Waiting for {controller}/{name} to become ready...")
    if controller == "deployment":
        try:
            run(
                [
                    "kubectl",
                    "wait",
                    "--for=condition=Available",
                    f"{controller}/{name}",
                    f"--timeout={timeout}s",
                ]
            )
        except subprocess.CalledProcessError:
            print(f"⚠️ Timeout waiting for Deployment {name}")
            sys.exit(1)
    elif controller == "statefulset":
        try:
            run(
                [
                    "kubectl",
                    "rollout",
                    "status",
                    f"{controller}/{name}",
                    f"--timeout={timeout}s",
                ]
            )
        except subprocess.CalledProcessError:
            print(f"⚠️ Timeout waiting for StatefulSet {name}")
            sys.exit(1)
    else:
        print(f"❌ Unknown controller type: {controller}. Aborting...")
        sys.exit(1)


def main(deployment_config_path: str, data_dir: str):
    print("Starting script")
    config.load_kube_config()
    services = load_services(deployment_config_path)

    resources_to_wait_for = []

    for service_name, controller in reversed(services):
        service_name_lower = service_name.lower()
        controller_lower = controller.lower()
        resource_name = f"sequencer-{service_name_lower}-{controller_lower}"

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

        copy_state(pod_name, data_dir)
        delete_pod(pod_name)

        # Add the resource to the list of resources to wait for.
        resources_to_wait_for.append((controller_lower, resource_name))

    # After deleting all pods, wait for all resources.
    print("\n⏳ Waiting for all resources to become ready...\n")
    for controller, resource_name in resources_to_wait_for:
        print(f"⏳ Waiting for {controller}/{resource_name} to become ready...")
        wait_for_resource(controller, resource_name)
        print(f"✅ {controller}/{resource_name} is ready!")

    print("\n✅ All services are ready!")


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(
        description="Copy state and restart sequencer pods based on a deployment config."
    )

    parser.add_argument(
        "--deployment_config_path", help="Path to the deployment config JSON file"
    )

    parser.add_argument(
        "--data-dir",
        default="./output/data/node_0",
        help="Directory containing the state to copy into pods (default: ./output/data/node_0)",
    )

    args = parser.parse_args()
    main(args.deployment_config_path, args.data_dir)
