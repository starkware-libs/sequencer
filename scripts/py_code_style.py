#!/usr/bin/env python3.9

import argparse
import os
import subprocess

from merge_branches import FINAL_BRANCH, MERGE_PATHS_FILE, load_merge_paths

CURRENT_DIR = os.path.dirname(__file__)
ROOT_PROJECT_DIR = os.path.abspath(os.path.join(CURRENT_DIR, ".."))
PARENT_BRANCH = open(os.path.join(CURRENT_DIR, "parent_branch.txt")).read().strip()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run Python code style checks.")
    parser.add_argument(
        "--fix", action="store_true", help="Fix mode. Runs only fixable checks, in fix mode."
    )
    return parser.parse_args()


def run_black(fix: bool):
    command = ["black", "-l", "100", "-t", "py37", ROOT_PROJECT_DIR]
    if not fix:
        command += ["--check", "--diff", "--color"]
    subprocess.check_output(command)


def run_isort(fix: bool):
    command = ["isort", "--settings-path", ROOT_PROJECT_DIR, ROOT_PROJECT_DIR]
    if not fix:
        command.append("-c")
    subprocess.check_output(command)


def verify_linear_path():
    """
    Verify the merge paths JSON describes a linear merge path.
    """
    merge_paths = load_merge_paths()
    src_dst_iter = iter(merge_paths.items())
    (oldest_branch, prev_dst_branch) = next(src_dst_iter)
    assert (
        oldest_branch not in merge_paths.values()
    ), f"Oldest branch '{oldest_branch}' cannot be a destination branch."

    for src_branch, dst_branch in src_dst_iter:
        assert prev_dst_branch == src_branch, (
            f"Since the merge graph is linear, the source branch '{src_branch}' must be the same "
            f"as the previous destination branch, which is '{prev_dst_branch}'. Check out "
            f"{MERGE_PATHS_FILE}."
        )
        prev_dst_branch = dst_branch

    assert (
        prev_dst_branch == FINAL_BRANCH
    ), f"The last destination is '{prev_dst_branch}' but must be '{FINAL_BRANCH}'."


def verify_parent_branch_is_on_path():
    merge_paths = load_merge_paths()
    known_branches = set(merge_paths.keys()) | set(merge_paths.values())
    assert PARENT_BRANCH in known_branches, (
        f"Parent branch '{PARENT_BRANCH}' is not on the merge path (branches in merge path: "
        f"{known_branches})."
    )


def merge_branches_checks():
    verify_linear_path()
    verify_parent_branch_is_on_path()


def main():
    args = parse_args()
    run_black(fix=args.fix)
    run_isort(fix=args.fix)
    if not args.fix:
        # Unfixable checks.
        merge_branches_checks()


if __name__ == "__main__":
    main()
