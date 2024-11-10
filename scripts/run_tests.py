#!/bin/env python3

import argparse
from enum import Enum
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


# Enum of base commands.
class BaseCommand(Enum):
    TEST = "test"
    CODECOV = "codecov"
    RUSTFMT = "rustfmt"
    CLIPPY = "clippy"
    DOC = "doc"

    def cmd(self, crates: Set[str]) -> List[str]:
        package_args = []
        operands = ["--", "-Dwarnings"]  # "--" must be the first element.
        for package in crates:
            package_args.extend(["--package", package])

        if self == BaseCommand.TEST:
            return ["cargo", "test"] + package_args + operands
        elif self == BaseCommand.CODECOV:
            return [
                "cargo",
                "llvm-cov",
                "--codecov",
                "-r",
                "--output-path",
                "codecov.json",
            ] + package_args
        elif self == BaseCommand.RUSTFMT:
            fmt_args = package_args if len(package_args) > 0 else ["--all"]
            return ["scripts/rust_fmt.sh"] + fmt_args + ["--", "--check"]
        elif self == BaseCommand.CLIPPY:
            clippy_args = package_args if len(package_args) > 0 else ["--workspace"]
            return ["cargo", "clippy"] + clippy_args + operands
        elif self == BaseCommand.DOC:
            doc_args = package_args if len(package_args) > 0 else ["--workspace"]
            return (
                ["cargo", "doc", "-r", "--document-private-items", "--no-deps"]
                + doc_args
                + operands
            )

        raise NotImplementedError(f"Command {self} not implemented.")


def packages_to_test_due_to_global_changes(files: List[str]) -> Set[str]:
    if len(set(files).intersection(ALL_TEST_TRIGGERS)) > 0:
        return set(get_workspace_tree().keys())
    return set()


def test_crates(crates: Set[str], base_command: BaseCommand):
    """
    Runs tests for the given crates.
    If no crates provided, runs tests for all crates.
    """
    args = []
    for package in crates:
        args.extend(["--package", package])

    # If crates is empty (i.e. changes_only is False), all packages will be tested (no args).
    cmd = base_command.cmd(crates=crates)

    print("Running tests...")
    print(cmd, flush=True)
    subprocess.run(cmd, check=True)
    print("Tests complete.")


def run_test(
    changes_only: bool, commit_id: Optional[str], base_command: bool, include_dependencies: bool
):
    """
    Runs tests.
    If changes_only is True, only tests packages that have been modified; if no packages have been
    modified, no tests are run. If changes_only is False, tests all packages.
    If commit_id is provided, compares against that commit; otherwise, compares against HEAD.
    """
    if not changes_only:
        assert not include_dependencies, "include_dependencies can only be set with changes_only."
    tested_packages = set()
    if changes_only:
        local_changes = get_local_changes(".", commit_id=commit_id)
        modified_packages = get_modified_packages(local_changes)

        if include_dependencies:
            for p in modified_packages:
                deps = get_package_dependencies(p)
                tested_packages.update(deps)
            print(
                f"Running tests for {tested_packages} (due to modifications in "
                f"{modified_packages})."
            )
        else:
            print(f"Running tests for modified crates {modified_packages}.")
            tested_packages = modified_packages

        # Add global-triggered packages.
        extra_packages = packages_to_test_due_to_global_changes(files=local_changes)
        print(f"Running tests for global-triggered packages {extra_packages}")
        tested_packages.update(extra_packages)
        if len(tested_packages) == 0:
            print("No changes detected.")
            return

    test_crates(crates=tested_packages, base_command=base_command)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Presubmit script.")
    parser.add_argument("--changes_only", action="store_true", help="Only test modified crates.")
    parser.add_argument("--commit_id", type=str, help="GIT commit ID to compare against.")
    parser.add_argument(
        "--command",
        required=True,
        choices=[cmd.value for cmd in BaseCommand],
        help="Code inspection command to run.",
    )
    parser.add_argument(
        "--include_dependencies",
        action="store_true",
        help="Dependencies of modified crates are also tested. Can only be set with changes_only.",
    )
    return parser.parse_args()


def main():
    args = parse_args()
    run_test(
        changes_only=args.changes_only,
        commit_id=args.commit_id,
        base_command=BaseCommand(args.command),
        include_dependencies=args.include_dependencies,
    )


if __name__ == "__main__":
    main()
