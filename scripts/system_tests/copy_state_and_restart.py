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
    print(f"📥 Copying state contents from '{data_dir}' to pod {pod_name}:/data ...")

    if not os.path.isdir(data_dir):
        print(f"❌ '{data_dir}' is not a valid directory")
        sys.exit(1)

    batcher_path = os.path.join(data_dir, "batcher/")
    batcher_target = f"{pod_name}:/data/"
    state_sync_path = os.path.join(data_dir, "state_sync/")
    state_sync_target = f"{pod_name}:/data"
    class_manager_path = os.path.join(data_dir, "class_manager/")
    class_manager_target = f"{pod_name}:/data"
    print(f"📦 Copying batcher: {batcher_path} → {batcher_target}")
    run(
        [
            "kubectl",
            "cp",
            f"{data_dir}/batcher/",
            f"{pod_name}:/data/",
            "--retries=3",
        ],
        check=True,
    )
    print(f"📦 Copying state_sync: {state_sync_path} → {state_sync_target}")
    run(
        [
            "kubectl",
            "cp",
            state_sync_path,
            state_sync_target,
            "--retries=3",
        ],
        check=True,
    )
    print(f"📦 Copying class_manager: {class_manager_path} → {class_manager_target}")
    run(
        [
            "kubectl",
            "cp",
            class_manager_path,
            class_manager_target,
            "--retries=3",
        ],
        check=True,
    )
    # print(f"🔍 Listing files in /data of pod {pod_name}...")
    # subprocess.run(
    #     ["kubectl", "exec", pod_name, "--", "ls", "-l", "/data"],
    #     check=True,
    #     capture_output=True,
    #     text=True,
    # )
    # for item in os.listdir(data_dir):
    #     item_path = os.path.join(data_dir, item)
    #     target_path = f"{pod_name}:/data/{item}"
    #     print(f"📦 Copying: {item_path} → {target_path}")
    #     try:
    #         run(
    #             [
    #                 "kubectl",
    #                 "cp",
    #                 item_path,
    #                 target_path,
    #                 "--retries=3",
    #             ],
    #             check=True,
    #         )
    #     except subprocess.CalledProcessError as e:
    #         print(f"❌ Failed to copy '{item}': {e}")
    #         run(["kubectl", "describe", "pod", pod_name], check=False)
    #         sys.exit(1)


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


def validate_remote_data(
    pod_name: str, local_file_sizes: Dict[str, int], data_dir: str
) -> None:
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
                "%n %s",
                "{}",
                "+",
            ],
            capture_output=True,
        )
        remote_lines = result.stdout.strip().splitlines()
    except subprocess.CalledProcessError as e:
        print(f"❌ Failed to list files in pod {pod_name}: {e}")
        sys.exit(1)

    # Determine the subdirectory name that may be present in the pod's /data
    data_dir_basename = os.path.basename(os.path.normpath(data_dir))

    remote_file_sizes: Dict[str, int] = {}
    for line in remote_lines:
        try:
            full_path, size_str = line.rsplit(" ", 1)

            # Try to trim /data/{basename}/... to align with local paths
            if full_path.startswith(f"/data/{data_dir_basename}/"):
                rel_path = os.path.relpath(full_path, f"/data/{data_dir_basename}")
            else:
                rel_path = os.path.relpath(full_path, "/data")

            remote_file_sizes[rel_path] = int(size_str)
        except Exception as e:
            print(f"⚠️ Skipping invalid line: {line} ({e})")

    all_matched = True

    for rel_path, expected_size in local_file_sizes.items():
        actual_size = remote_file_sizes.get(rel_path)
        if actual_size is None:
            print(f"❌ MISSING: {rel_path} not found in /data on pod {pod_name}")
            all_matched = False
        elif actual_size != expected_size:
            print(
                f"❌ SIZE MISMATCH: {rel_path} - local: {expected_size} bytes, remote: {actual_size} bytes"
            )
            all_matched = False
        else:
            print(f"✅ {rel_path} matches (size: {expected_size} bytes)")

    extra_files = set(remote_file_sizes) - set(local_file_sizes)
    if extra_files:
        print("\n⚠️ Extra files present in pod /data that are not in local data:")
        for path in sorted(extra_files):
            print(f"  - {path} ({remote_file_sizes[path]} bytes)")

    if not all_matched:
        print(
            "\n❌ Validation failed: Some files are missing or have mismatched sizes."
        )
        # sys.exit(1)

    print("🎉 Remote validation successful: All files match in name and size.\n")


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

        copy_state(pod_name=pod_name, data_dir=data_dir)
        delete_pod(pod_name=pod_name)
        resources_to_wait_for.append((controller_lower, resource_name, pod_name))

    print("\n⏳ Waiting for all resources to become ready...\n")

    for controller, resource_name, pod_name in resources_to_wait_for:
        wait_for_resource(controller=controller, name=resource_name)
        print(f"✅ {controller}/{resource_name} is ready!")
        # validate_remote_data(pod_name, local_file_sizes, data_dir)

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
