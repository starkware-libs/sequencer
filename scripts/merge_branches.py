#!/usr/bin/env python3.9

"""
Merge a branch into another branch. Example usage:
```
scripts/merge_branches.py --src main-v0.13.0
```
"""

import argparse
import json
import os
import subprocess
import time
from typing import Dict, List, Optional
from utils import run_command

FINAL_BRANCH = "main"
MERGE_PATHS_FILE = "scripts/merge_paths.json"
FILES_TO_PRESERVE = {"scripts/parent_branch.txt"}


def load_merge_paths() -> Dict[str, str]:
    return json.load(open(MERGE_PATHS_FILE))


def get_dst_branch(src_branch: str, dst_branch_override: Optional[str]) -> str:
    if dst_branch_override is not None:
        return dst_branch_override
    assert (
        src_branch.replace("origin/", "") != FINAL_BRANCH
    ), f"{FINAL_BRANCH} has no default destination branch."

    return load_merge_paths()[src_branch]


def srcdiff(source_branch: str, destination_branch: Optional[str], files: List[str]):
    destination_branch = get_dst_branch(
        src_branch=source_branch, dst_branch_override=destination_branch
    )
    files_line = " ".join(files)
    run_command(
        f"git diff $(git merge-base origin/{source_branch} origin/{destination_branch}) "
        f"origin/{source_branch} {files_line}"
    )


def dstdiff(source_branch: str, destination_branch: Optional[str], files: List[str]):
    destination_branch = get_dst_branch(
        src_branch=source_branch, dst_branch_override=destination_branch
    )
    files_line = " ".join(files)
    run_command(
        f"git diff $(git merge-base origin/{source_branch} origin/{destination_branch}) "
        f"origin/{destination_branch} {files_line}"
    )


def verify_gh_client_status():
    try:
        run_command("gh --version")
    except subprocess.CalledProcessError:
        print(
            "GitHub CLI not found. Please install it from "
            "https://github.com/cli/cli/blob/trunk/docs/install_linux.md#installing-gh-on-linux-and-bsd"
        )
        exit(1)
    try:
        run_command("gh auth status")
    except subprocess.CalledProcessError:
        print(
            "GitHub CLI not authenticated. Please authenticate using `gh auth login` "
            "and follow the instructions."
        )
        exit(1)


def current_git_conflictstyle() -> Optional[str]:
    try:
        output = run_command("git config --get merge.conflictstyle")
        assert len(output) == 1
        return output[0]
    except subprocess.CalledProcessError:
        return None


def merge_branches(src_branch: str, dst_branch: Optional[str], auto_delete_from_dst: bool):
    """
    Merge source branch into destination branch.
    If no destination branch is passed, the destination branch is taken from state on repo.
    """
    verify_gh_client_status()
    user = os.environ["USER"]
    dst_branch = get_dst_branch(src_branch=src_branch, dst_branch_override=dst_branch)

    merge_branch = f"{user}/merge-{src_branch}-into-{dst_branch}-{int(time.time())}"
    print(f"Source branch: {src_branch}")
    print(f"Destination branch: {dst_branch}\n")

    run_command("git fetch")
    run_command(f"git checkout origin/{dst_branch}")
    run_command(f"git checkout -b {merge_branch}")

    print("Merging...")
    conflictstyle = current_git_conflictstyle()
    run_command("git config merge.conflictstyle diff3")
    run_command(f"git merge origin/{src_branch}", allow_error=True)
    if conflictstyle is None:
        run_command("git config --unset merge.conflictstyle")
    else:
        run_command(f"git config merge.conflictstyle {conflictstyle}")

    run_command(f"git checkout origin/{dst_branch} {' '.join(FILES_TO_PRESERVE) }")

    included_types_as_conflicts = ["UU", "AA"]
    if auto_delete_from_dst:
        included_types_as_conflicts.append("UD")

    grep_re_expression = "|".join(f"^{t}" for t in included_types_as_conflicts)
    conflicts_file = "/tmp/conflicts"
    find_conflicts_cmd = f"git status -s | grep -E \"{grep_re_expression}\" | awk '{{ print $2 }}' | tee {conflicts_file}"

    run_command(find_conflicts_cmd)

    conflicts = [line.strip() for line in open(conflicts_file).readlines() if line.strip() != ""]
    conflict_line = " ".join(conflicts)
    run_command(f"git add {conflict_line}", allow_error=True)
    print("Committing conflicts...")
    if len(conflicts) == 0:
        run_command(
            f'git commit --allow-empty -m "No conflicts in {src_branch} -> {dst_branch} merge, '
            'this commit is for any change needed to pass the CI."'
        )
    else:
        run_command(
            f'git commit -m "chore: merge branch {src_branch} into {dst_branch} (with conflicts)"'
        )

    print("Pushing...")
    run_command(f"git push --set-upstream origin {merge_branch}")
    (merge_base,) = run_command(f"git merge-base origin/{src_branch} origin/{dst_branch}")

    print("Creating PR...")
    run_command(
        f'gh pr create --base {dst_branch} --title "Merge {src_branch} into {dst_branch}" '
        '--body ""'
    )

    if len(conflicts) != 0:
        compare = "https://github.com/starkware-libs/sequencer/compare"
        comment_file_path = "/tmp/comment.XXXXXX"
        with open(comment_file_path, "w") as comment_file:
            for conflict in conflicts:
                (filename_hash,) = run_command(f"echo -n {conflict} | sha256sum | cut -d' ' -f1")
                comment_file.write(
                    f"[Src]({compare}/{merge_base}..{src_branch}#diff-{filename_hash}) "
                    f"[Dst]({compare}/{merge_base}..{dst_branch}#diff-{filename_hash}) "
                    f"{conflict}\n"
                )
        run_command(f"gh pr comment -F {comment_file_path}")
        os.remove(comment_file_path)

    os.remove(conflicts_file)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Merge a branch into another branch.")
    parser.add_argument("--src", type=str, help="The source branch to merge.", required=True)
    parser.add_argument(
        "--dst",
        type=str,
        default=None,
        help=(
            "The destination branch to merge into. If no branch explicitly provided, uses the "
            f"destination branch registered for the source branch in {MERGE_PATHS_FILE}."
        ),
    )
    parser.add_argument(
        "--auto-delete-from-dst",
        default=False,
        action="store_true",
        help="If files were updated on source but deleted on destination, delete the files.",
    )
    args = parser.parse_args()

    merge_branches(
        src_branch=args.src, dst_branch=args.dst, auto_delete_from_dst=args.auto_delete_from_dst
    )
