#!/bin/env python3

"""
Enforces that `#[expect(unused*)]` is used instead of `#[allow(unused*)]` in Rust source files.

With `#[expect(...)]`, the compiler warns (and CI errors via `-D warnings`) when the expectation is
unfulfilled — i.e., when the item is no longer actually unused. This ensures allow-unused annotations
stay in sync with reality.
"""

import argparse
import os
import re
from typing import Optional

from tests_utils import get_local_changes

# Matches #[allow(...)] containing any bare unused* lint, regardless of position in the lint list.
# Handles variants like: unused, unused_imports, unused_variables, unused_macro_rules, etc.
# Excludes qualified lints like clippy::unused_async.
ALLOW_UNUSED_PATTERN = re.compile(r"#!?\[allow\([^]]*(?<![:\w])unused\w*[^]]*\)]")


def check_allow_unused(file_path: str) -> bool:
    """
    Checks that the file does not contain `#[allow(unused*)]` attributes.

    Returns:
        bool: True if the file is clean, False if violations were found.
    """
    violations = []
    try:
        with open(file_path, "r") as file:
            for line_number, line in enumerate(file, start=1):
                if ALLOW_UNUSED_PATTERN.search(line):
                    violations.append((file_path, line_number, line.strip()))
    except Exception as e:
        print(f"Error while reading file {file_path}: {e}")
        raise e

    if violations:
        print(f"{len(violations)} #[allow(unused*)] found (use #[expect(unused*)] instead):")
        for file_path, line_number, line in violations:
            print(f"  {file_path}:{line_number}: '{line}'")
        return False
    return True


def enforce_no_allow_unused(commit_id: Optional[str]):
    """
    Enforce that modified Rust files use `#[expect(unused*)]` instead of `#[allow(unused*)]`.
    If commit_id is provided, compares against that commit; otherwise, compares against HEAD.
    """
    local_changes = get_local_changes(".", commit_id=commit_id)
    rust_files = [f for f in local_changes if f.endswith(".rs") and os.path.isfile(f)]
    print(f"Checking for #[allow(unused*)] in modified Rust files: {rust_files}.")
    successful_validation = all(check_allow_unused(file_path) for file_path in rust_files)
    assert successful_validation, (
        "Found #[allow(unused*)]. Use #[expect(unused*)] instead so the compiler warns when the "
        "item is no longer unused."
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Enforce #[expect(unused*)] instead of #[allow(unused*)]."
    )
    parser.add_argument("--commit_id", type=str, help="GIT commit ID to compare against.")
    return parser.parse_args()


def main():
    args = parse_args()
    enforce_no_allow_unused(commit_id=args.commit_id)


if __name__ == "__main__":
    main()
