#!/bin/env python3

import argparse
import subprocess
from typing import List, Set, Optional
from tests_utils import (
    get_workspace_tree,
    get_local_changes,
    get_modified_packages,
    get_package_dependencies,
)

# Set of files which - if changed - should trigger tests for all packages.
ALL_TEST_TRIGGERS: Set[str] = {"Cargo.toml", "Cargo.lock"}


def packages_to_test_due_to_global_changes(files: List[str]) -> Set[str]:
    if len(set(files).intersection(ALL_TEST_TRIGGERS)) > 0:
        return set(get_workspace_tree().keys())
    return set()


def run_test(changes_only: bool, commit_id: Optional[str]):
    local_changes = get_local_changes(".", commit_id=commit_id)
    modified_packages = get_modified_packages(local_changes)
    args = []
    tested_packages = set()
    if changes_only:
        for p in modified_packages:
            deps = get_package_dependencies(p)
            tested_packages.update(deps)
        print(f"Running tests for {tested_packages} (due to modifications in {modified_packages}).")
        # Add global-triggered packages.
        extra_packages = packages_to_test_due_to_global_changes(files=local_changes)
        print(f"Running tests for global-triggered packages {extra_packages}")
        tested_packages.update(extra_packages)
        if len(tested_packages) == 0:
            print("No changes detected.")
            return

    for package in tested_packages:
        args.extend(["--package", package])

    # If tested_packages is empty (i.e. changes_only is False), all packages will be tested (no
    # args).
    cmd = ["cargo", "test"] + args

    print("Running tests...")
    print(cmd, flush=True)
    subprocess.run(cmd, check=True)
    print("Tests complete.")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Presubmit script.")
    parser.add_argument("--changes_only", action="store_true")
    parser.add_argument("--commit_id", type=str, help="GIT commit ID to compare against.")
    return parser.parse_args()


def main():
    args = parse_args()
    run_test(changes_only=args.changes_only, commit_id=args.commit_id)


if __name__ == "__main__":
    main()
