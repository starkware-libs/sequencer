#!/bin/env python3

import argparse
import os
import re
from typing import Optional

from tests_utils import get_local_changes


def validate_todo_format(file_path: str) -> bool:
    """
    Validates that all TODO comments in the file are formatted as TODO(X), where X is a non-empty
    string of characters.

    Args:
        file_path (str): Path to the file to be checked.

    Returns:
        bool: True if all TODO comments are valid, False otherwise.
    """
    # Skip this current file, as the following regex definition itself is detected as an unformatted
    # TODO comment.
    if os.path.relpath(__file__) == file_path:
        return True

    # Matches a comment indicator (// or #), any set characters, and the TODO literal.
    comment_todo_pattern = re.compile(r"(\/\/|#).*?TODO")
    # Matches a comment indicator (// or #), an optional single space, the TODO literal,
    # parenthesis bounding a non-empty string (owner name), and a colon.
    required_comment_todo_pattern = re.compile(r"(\/\/|#) ?TODO\([^)]+\):")
    invalid_todos = []
    try:
        with open(file_path, "r") as file:
            for line_number, line in enumerate(file, start=1):
                if comment_todo_pattern.search(line):
                    if not required_comment_todo_pattern.search(line):
                        invalid_todos.append((file_path, line_number, line.strip()))
    except Exception as e:
        # Make sure to report which file caused the error before re-raising the exception.
        print(f"Error while reading file {file_path}: {e}")
        raise e
    if len(invalid_todos) > 0:
        print(f"{len(invalid_todos)} invalid TODOs found.")
        for file_path, line_number, line in invalid_todos:
            print(f"{file_path}:{line_number}: '{line}'")
        return False
    return True


def enforce_named_todos(commit_id: Optional[str]):
    """
    Enforce TODO comments format.
    If commit_id is provided, compares against that commit; otherwise, compares against HEAD.
    """

    local_changes = get_local_changes(".", commit_id=commit_id)
    print(f"Enforcing TODO format on modified files: {local_changes}.")
    successful_validation = all(
        validate_todo_format(file_path)
        for file_path in local_changes
        if os.path.isfile(file_path) and not file_path.endswith(".bin")
    )
    assert successful_validation, "Found invalid TODOs"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Enforcing all TODO comments are properly named.")
    parser.add_argument("--commit_id", type=str, help="GIT commit ID to compare against.")
    return parser.parse_args()


def main():
    args = parse_args()
    enforce_named_todos(commit_id=args.commit_id)


if __name__ == "__main__":
    main()
