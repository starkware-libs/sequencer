#!/bin/env python3

import argparse
import fnmatch
import os
import sys
from typing import List, Set

from tests_utils import get_local_changes, get_tested_packages


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
    parser = argparse.ArgumentParser(
        description="Check if a test should be triggered based on code changes."
    )
    parser.add_argument(
        "--commit_id", type=str, required=True, help="GIT commit ID to compare against."
    )
    parser.add_argument(
        "--output_file",
        type=str,
        required=True,
        help="Path to file that will contain the result ('true' or 'false').",
    )
    parser.add_argument(
        "--crate_triggers",
        type=str,
        default="",
        help="Comma-separated list of crates that should trigger the test.",
    )
    parser.add_argument(
        "--path_triggers",
        type=str,
        default="",
        help="Comma-separated list of file/path patterns that should trigger the test.",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()

    crate_triggers: Set[str] = set(filter(None, args.crate_triggers.split(",")))
    path_triggers: List[str] = list(filter(None, args.path_triggers.split(",")))

    tested = get_tested_packages(
        changes_only=True, commit_id=args.commit_id, include_dependencies=True
    )

    if tested is None:
        tested = set()

    crate_trigger = any(crate in crate_triggers for crate in tested)
    print(f"crate_trigger: {crate_trigger}", file=sys.stderr)

    file_trigger = is_file_triggered(args.commit_id, path_triggers)
    print(f"file_trigger: {file_trigger}", file=sys.stderr)

    should_run = crate_trigger or file_trigger

    with open(args.output_file, "w", encoding="utf-8") as f:
        f.write("true" if should_run else "false")


if __name__ == "__main__":
    main()
