#!/bin/env python3

import argparse
from calendar import c
import subprocess
from typing import List, Set, Optional
from tests_utils import (
    get_local_changes,
    get_modified_packages,
)


def run_codecov(changes_only: bool, commit_id: Optional[str]):

    local_changes = get_local_changes(".", commit_id=commit_id)
    modified_packages = set(get_modified_packages(local_changes))

    if changes_only and len(modified_packages) == 0:
        print("No changes detected.")
        return

    print(f"Running code coverage for {modified_packages}.")

    args = []

    for package in modified_packages:
        args.extend(["--package", package])

    cmd = ["cargo", "llvm-cov", "--codecov", "-r", "--output-path", "codecov.json"] + args

    print("Running code coverage...")
    print(cmd, flush=True)
    subprocess.run(cmd, check=True)
    print("Code coverage complete.")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Code coverage script.")
    parser.add_argument("--changes_only", action="store_true")
    parser.add_argument("--commit_id", type=str, help="GIT commit ID to compare against.")
    return parser.parse_args()


def main():
    args = parse_args()
    run_codecov(changes_only=args.changes_only, commit_id=args.commit_id)


if __name__ == "__main__":
    main()
