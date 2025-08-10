#!/bin/env python3

import argparse
import subprocess
from enum import Enum
from typing import List, Optional, Set
import os

from tests_utils import get_tested_packages

# Set of crates which - if changed - should trigger the integration tests.
INTEGRATION_TEST_CRATE_TRIGGERS: Set[str] = {"apollo_integration_tests"}

# Set of crates which - if changed - should trigger re-running the integration tests with `cairo_native` feature.
CAIRO_NATIVE_CRATE_TRIGGERS: Set[str] = {"blockifier"}

# Sequencer node binary name.
SEQUENCER_BINARY_NAME: str = "apollo_node"

# List of sequencer node integration test binary names. Stored as a list to maintain order.
SEQUENCER_INTEGRATION_TEST_NAMES: List[str] = [
    # TODO(NoamP): enable this once fixed.
    # "integration_test_restart_flow",
]
NIGHTLY_ONLY_SEQUENCER_INTEGRATION_TEST_NAMES: List[str] = [
    "integration_test_positive_flow",
    "integration_test_restart_flow",
    "integration_test_revert_flow",
    "integration_test_central_and_p2p_sync_flow",
]


# Enum of base commands.
class BaseCommand(Enum):
    TEST = "test"
    CLIPPY = "clippy"
    DOC = "doc"
    INTEGRATION = "integration"
    MIRI = "miri"

    def cmds(self, crates: Set[str], is_nightly: bool) -> List[List[str]]:
        package_args = []
        for package in crates:
            package_args.extend(["--package", package])

        if self == BaseCommand.TEST:
            return [["cargo", "test"] + package_args]
        elif self == BaseCommand.MIRI:
            nightly_rust_version = os.environ.get("NIGHTLY_RUST_VERSION")
            return [["cargo", f"+{nightly_rust_version}" ,"miri", "test", "--lib"] + package_args]
        elif self == BaseCommand.CLIPPY:
            clippy_args = package_args if len(package_args) > 0 else ["--workspace"]
            return [["cargo", "clippy"] + clippy_args + ["--all-targets", "--all-features"]]
        elif self == BaseCommand.DOC:
            doc_args = package_args if len(package_args) > 0 else ["--workspace"]
            return [["cargo", "doc", "--document-private-items", "--no-deps"] + doc_args]
        elif self == BaseCommand.INTEGRATION:
            # Do nothing if integration tests should not be triggered; on nightly, run the tests.
            if INTEGRATION_TEST_CRATE_TRIGGERS.isdisjoint(crates) and not is_nightly:
                print(f"Skipping sequencer integration tests.")
                return []

            integration_test_names_to_run = (
                NIGHTLY_ONLY_SEQUENCER_INTEGRATION_TEST_NAMES
                if is_nightly
                else SEQUENCER_INTEGRATION_TEST_NAMES
            )

            print(f"Composing sequencer integration test commands.")

            def build_cmds(with_feature: bool) -> List[List[str]]:
                feature_flag = (
                    ["--features", "cairo_native"] if (with_feature and is_nightly) else []
                )
                # Commands to build the node and all the test binaries.
                build_cmds = [
                    ["cargo", "build", "--bin", binary_name] + feature_flag
                    for binary_name in [SEQUENCER_BINARY_NAME] + integration_test_names_to_run
                ]
                return build_cmds

            # Commands to run the test binaries.
            run_cmds = [
                [f"./target/debug/{test_binary_name}"]
                for test_binary_name in integration_test_names_to_run
            ]

            cmds_no_feat = build_cmds(with_feature=False) + run_cmds

            # Only run cairo_native feature if the blockifier crate is modified, and in nightly.
            if CAIRO_NATIVE_CRATE_TRIGGERS.isdisjoint(crates) and not is_nightly:
                return cmds_no_feat

            print("Composing sequencer integration test commands with cairo_native feature.")
            cmds_with_feat = build_cmds(with_feature=True) + run_cmds
            return cmds_no_feat + cmds_with_feat

        raise NotImplementedError(f"Command {self} not implemented.")


def test_crates(crates: Set[str], base_command: BaseCommand, is_nightly: bool):
    """
    Runs tests for the given crates.
    If no crates provided, runs tests for all crates.
    """
    # If crates is empty (i.e. changes_only is False), all packages will be tested (no args).
    cmds = base_command.cmds(crates=crates, is_nightly=is_nightly)

    print("Executing test commands...")
    for cmd in cmds:
        print(cmd, flush=True)
        subprocess.run(cmd, check=True)
    print("Tests complete.")


def run_test(
    changes_only: bool,
    commit_id: Optional[str],
    base_command: BaseCommand,
    include_dependencies: bool,
    is_nightly: bool,
):
    """
    Runs tests.
    If changes_only is True, only tests packages that have been modified; if no packages have been
    modified, no tests are run. If changes_only is False, tests all packages.
    If commit_id is provided, compares against that commit; otherwise, compares against HEAD.
    """
    tested_packages = get_tested_packages(
        changes_only=changes_only,
        commit_id=commit_id,
        include_dependencies=include_dependencies,
    )
    if changes_only and len(tested_packages) == 0:
        print("No changes detected.")
        return
    test_crates(crates=tested_packages, base_command=base_command, is_nightly=is_nightly)


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
    parser.add_argument(
        "--is_nightly",
        action="store_true",
        default=False,
        help="Indicates if the run should be performed in nightly mode. Defaults to False.",
    )
    return parser.parse_args()


def main():
    args = parse_args()
    run_test(
        changes_only=args.changes_only,
        commit_id=args.commit_id,
        base_command=BaseCommand(args.command),
        include_dependencies=args.include_dependencies,
        is_nightly=args.is_nightly,
    )


if __name__ == "__main__":
    main()
