import re


import argparse
from typing import Optional
from tests_utils import (
    get_local_changes,
)

def validate_todo_format(file_path) -> bool:
    """
    Validates that all TODOs in the file are formatted as TODO(X),
    where X is a non-empty string of characters.

    Args:
        file_path (str): Path to the file to be checked.

    Returns:
        bool: True if all TODOs are valid, False otherwise.
    """
    todo_pattern = re.compile(r"TODO\([^)]+\)")
    no_invalid_todos = True
    with open(file_path, "r") as file:
        for line_number, line in enumerate(file, start=1):
            if "TODO" in line:
                match = todo_pattern.search(line)
                if not match:
                    no_invalid_todos = False
                    print((line_number, line.strip()))
    return no_invalid_todos



def enforce_named_todos(
    commit_id: Optional[str], 
):
    """
    Enforce TODO comments format.
    If commit_id is provided, compares against that commit; otherwise, compares against HEAD.
    """

    local_changes = get_local_changes(".", commit_id=commit_id)
    local_changes_rust_src_files = [file for file in local_changes if file.endswith(".rs")]
    print(f"Enforcing TODO format on modified files {local_changes_rust_src_files}.")
    successful_validation = all(validate_todo_format(file_path) for file_path in local_changes if file_path.endswith(".rs"))
    assert successful_validation, "Found invalid TODOs"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Presubmit script.")
    parser.add_argument("--commit_id", type=str, help="GIT commit ID to compare against.")
    return parser.parse_args()


def main():
    args = parse_args()
    enforce_named_todos(
        commit_id=args.commit_id,
    )


if __name__ == "__main__":
    main()
