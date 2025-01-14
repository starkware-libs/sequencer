#!/bin/env python3

import subprocess
from tests_utils import get_workspace_packages


def build_without_features(package: str):
    cmd = f"cargo build --package {package}"
    print(f"Running '{cmd}'", flush=True)
    subprocess.run(cmd, check=True)


def build_with_all_features(package: str):
    cmd = f"cargo build --all-features --package {package}"
    print(f"Running '{cmd}'", flush=True)
    subprocess.run(cmd, check=True)


def main():
    packages = get_workspace_packages()
    print(f"Building {len(packages)} packages without features.", flush=True)
    for package in packages:
        build_without_features(package)
    print(f"Building {len(packages)} packages with all features.", flush=True)
    for package in packages:
        build_with_all_features(package)


if __name__ == "__main__":
    main()
