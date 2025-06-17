#!/bin/env python3

import argparse
import fnmatch
import os
import sys
from typing import List
from tests_utils import get_local_changes, get_tested_packages

SYSTEM_TEST_CRATE_TRIGGERS = {"apollo_node", "apollo_deployments"}
ADDITIONAL_TRIGGER_PATHS = [
    ".github/workflows/consolidated_system_test.yaml",
    "scripts/*.py",
    "scripts/system_tests/**/*.py",
]


def is_file_triggered(commit_id: str, trigger_patterns: List[str]) -> bool:
    """
    Returns True if any file changed since `commit_id` matches any of the given
    wildcard patterns in `trigger_patterns`.
    """
    changed_files = get_local_changes(".", commit_id)
    for changed in changed_files:
        normalized = changed.replace(os.sep, "/")
        for pattern in trigger_patterns:
            if fnmatch.fnmatch(normalized, pattern):
                return True
    return False


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Check system test trigger.")
    parser.add_argument(
        "--commit_id", type=str, help="GIT commit ID to compare against."
    )
    parser.add_argument(
        "--output_file",
        type=str,
        help="The file that will contain the trigger result.",
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

    should_run = crate_trigger or file_trigger

    with open(args.output_file, "w", encoding="utf-8") as f:
        f.write("true" if should_run else "false")


if __name__ == "__main__":
    main()
