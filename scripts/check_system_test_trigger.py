#!/bin/env python3

import argparse
import sys
from typing import List
from tests_utils import get_local_changes, get_tested_packages

SYSTEM_TEST_CRATE_TRIGGERS = {"apollo_node", "apollo_deployments"}
ADDITIONAL_TRIGGER_PATHS = [
    ".github/workflows/consolidated_system_test.yaml",
    "scripts/",
]


def is_file_triggered(commit_id: str, trigger_paths: List[str]) -> bool:
    """
    Returns True if any file changed since `commit_id` starts with any prefix in `trigger_paths`.
    """
    changed_files = get_local_changes(".", commit_id)
    return any(
        any(f.startswith(trigger) for trigger in trigger_paths) for f in changed_files
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Presubmit script.")
    parser.add_argument(
        "--commit_id", type=str, help="GIT commit ID to compare against."
    )
    return parser.parse_args()


def main():
    args = parse_args()
    tested = get_tested_packages(
        changes_only=True, commit_id=args.commit_id, include_dependencies=True
    )

    if tested is None:
        tested = set()

    crate_trigger = any(crate in SYSTEM_TEST_CRATE_TRIGGERS for crate in tested)
    print(f"crate_trigger: {crate_trigger}", file=sys.stderr)
    file_trigger = is_file_triggered(args.commit_id, ADDITIONAL_TRIGGER_PATHS)
    print(f"file_trigger: {file_trigger}", file=sys.stderr)

    if crate_trigger or file_trigger:
        print("true")
    else:
        print("false")


if __name__ == "__main__":
    main()
