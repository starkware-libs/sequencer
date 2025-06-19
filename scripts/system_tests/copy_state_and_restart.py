#!/bin/env python3

import json
import os
import subprocess
import sys
from typing import List, Tuple, Dict
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


def list_all_files(data_dir: str) -> None:
    print(f"📂 Listing all files in {data_dir}...")
    for root, _, files in os.walk(data_dir):
        for file in files:
            file_path = os.path.join(root, file)
            print(f"📄 {file_path}")

def summarize_directory(path: str) -> None:
    print(f"📂 Listing and summarizing all files in {path}...")
    total_size = 0
    file_count = 0
    size_map = {}
    for root, _, files in os.walk(path):
        for file in files:
            file_path = os.path.join(root, file)
            size = os.path.getsize(file_path)
            rel_path = os.path.relpath(file_path, path)
            print(f"  - {file_path} ({size} bytes)")
            size_map[rel_path] = size
            total_size += size
            file_count += 1
    print(f"📦 Total files: {file_count}, Total size: {total_size} bytes\n")
    return size_map


def copy_state(pod_name: str, data_dir: str) -> None:
    print(f"📥 Copying state data to pod {pod_name}...")
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
        run(["kubectl", "describe", "pod", pod_name], check=False)
        sys.exit(1)

def copy_state_tar(pod_name: str, data_dir: str) -> None:
    print(f"📥 Copying state data to pod {pod_name} using tar workaround...")
    try:
        tar_send = subprocess.Popen(
            ["tar", "cf", "-", "-C", data_dir, "."],
            stdout=subprocess.PIPE,
        )

        kubectl_exec = subprocess.Popen(
            ["kubectl", "exec", "-i", pod_name, "--", "tar", "xf", "-", "-C", "/data"],
            stdin=tar_send.stdout,
        )

        tar_send.stdout.close()
        tar_send.wait()
        kubectl_exec.wait()

        if tar_send.returncode != 0 or kubectl_exec.returncode != 0:
            raise subprocess.CalledProcessError(
                returncode=kubectl_exec.returncode or tar_send.returncode,
                cmd="tar | kubectl exec tar"
            )
    except subprocess.CalledProcessError as e:
        print(f"❌ Tar-based copy failed: {e}")
        run(["kubectl", "describe", "pod", pod_name], check=False)
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


def validate_remote_data(pod_name: str, local_file_sizes: Dict[str, int]) -> None:
    print(f"🔍 Validating /data in pod {pod_name}...")
    try:
        result = run(
            [
                "kubectl",
                "exec",
                pod_name,
                "--",
                "find",
                "/data",
                "-type",
                "f",
                "-exec",
                "stat",
                "-c",
                "%s %n",
                "{}",
                "+",
            ],
            capture_output=True,
        )
        output = result.stdout.strip()
        if not output:
            print(f"❌ /data is empty in pod {pod_name}")
            run(["kubectl", "describe", "pod", pod_name], check=False)
            sys.exit(1)

        mismatches = []
        total_remote_size = 0
        print(f"📦 Remote /data contents:")
        for line in output.splitlines():
            size_str, path = line.split(" ", 1)
            size = int(size_str)
            rel_path = path.replace("/data/", "")
            total_remote_size += size
            print(f"  - {path} ({size} bytes)")

            if rel_path in local_file_sizes:
                if local_file_sizes[rel_path] != size:
                    mismatches.append((rel_path, local_file_sizes[rel_path], size))
            else:
                print(f"⚠️ Extra file on pod: {rel_path}")

        print(f"\n📏 Total remote data size: {total_remote_size} bytes")

        if mismatches:
            print("❌ File size mismatches found:")
            for path, local_size, remote_size in mismatches:
                print(
                    f"  - {path}: local={local_size} bytes, remote={remote_size} bytes"
                )
            run(["kubectl", "describe", "pod", pod_name], check=False)
            sys.exit(1)

        print("✅ Remote data validated successfully.")
    except subprocess.CalledProcessError as e:
        print(f"❌ Failed to validate remote data in pod {pod_name}: {e}")
        run(["kubectl", "describe", "pod", pod_name], check=False)
        sys.exit(1)


def main(deployment_config_path: str, data_dir: str) -> None:
    config.load_kube_config()
    services: List[Tuple[str, str]] = load_services(deployment_config_path)

    resources_to_wait_for: List[Tuple[str, str, str]] = []
    local_file_sizes = summarize_directory(data_dir)

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

        print(f"✅ Pod found: {pod_name}")

        copy_state_tar(pod_name=pod_name, data_dir=data_dir)
        delete_pod(pod_name=pod_name)
        resources_to_wait_for.append((controller_lower, resource_name, pod_name))

    print("\n⏳ Waiting for all resources to become ready...\n")

    for controller, resource_name, pod_name in resources_to_wait_for:
        wait_for_resource(controller=controller, name=resource_name)
        print(f"✅ {controller}/{resource_name} is ready!")
        validate_remote_data(pod_name, local_file_sizes)

    print("\n✅ All services are ready and state has been validated!")


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
