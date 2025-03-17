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
ALL_TEST_TRIGGERS: Set[str] = {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml"}

# Set of crates which - if changed - should trigger the integration tests.
INTEGRATION_TEST_CRATE_TRIGGERS: Set[str] = {"starknet_integration_tests"}

# Sequencer node binary name.
SEQUENCER_BINARY_NAME: str = "starknet_sequencer_node"

# List of sequencer node integration test binary names. Stored as a list to maintain order.
SEQUENCER_INTEGRATION_TEST_NAMES: List[str] = [
    "integration_test_positive_flow",
    # TODO(Shahak/Noam.s): enable these when required
    # "integration_test_restart_flow",
    "integration_test_revert_flow",
    "integration_test_central_and_p2p_sync_flow",
]


# Enum of base commands.
class BaseCommand(Enum):
    TEST = "test"
    CLIPPY = "clippy"
    DOC = "doc"
    INTEGRATION = "integration"

    def cmds(self, crates: Set[str]) -> List[List[str]]:
        package_args = []
        for package in crates:
            package_args.extend(["--package", package])

        if self == BaseCommand.TEST:
            return [["cargo", "test"] + package_args]
        elif self == BaseCommand.CLIPPY:
            clippy_args = package_args if len(package_args) > 0 else ["--workspace"]
            return [["cargo", "clippy"] + clippy_args + ["--all-targets", "--all-features"]]
        elif self == BaseCommand.DOC:
            doc_args = package_args if len(package_args) > 0 else ["--workspace"]
            return [["cargo", "doc", "--document-private-items", "--no-deps"] + doc_args]
        elif self == BaseCommand.INTEGRATION:
            # Do nothing if integration tests should not be triggered.
            if INTEGRATION_TEST_CRATE_TRIGGERS.isdisjoint(crates):
                print(f"Skipping sequencer integration tests.")
                return []

            print(f"Composing sequencer integration test commands.")
            # Commands to build the node and all the test binaries.
            build_cmds = [
                ["cargo", "build", "--bin", binary_name]
                for binary_name in [SEQUENCER_BINARY_NAME] + SEQUENCER_INTEGRATION_TEST_NAMES
            ]
            # Port setup command, used to prevent port binding issues.
            port_cmds = [["sysctl", "-w", "net.ipv4.ip_local_port_range='40000 40200'"]]
            # Commands to run the test binaries.
            run_cmds = [
                [f"./target/debug/{test_binary_name}"]
                for test_binary_name in SEQUENCER_INTEGRATION_TEST_NAMES
            ]
            return build_cmds + port_cmds + run_cmds

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
    # If crates is empty (i.e. changes_only is False), all packages will be tested (no args).
    cmds = base_command.cmds(crates=crates)

    print("Executing test commands...")
    for cmd in cmds:
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
