#!/bin/env python3

import subprocess
from typing import List
from tests_utils import get_workspace_packages


def run_command(cmd: List[str]):
    print(f"Running '{' '.join(cmd)}'", flush=True)
    subprocess.run(cmd, check=True)


def build_without_features(package: str):
    run_command(cmd=["cargo", "build", "--package", package])


def build_with_all_features(package: str):
    run_command(cmd=["cargo", "build", "--all-features", "--package", package])


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
