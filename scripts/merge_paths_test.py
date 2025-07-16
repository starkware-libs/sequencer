from typing import Dict

import pytest
from merge_branches import FINAL_BRANCH, MERGE_PATHS_FILE, load_merge_paths


@pytest.fixture
def parent_branch() -> str:
    return open("scripts/parent_branch.txt").read().strip()


@pytest.fixture
def merge_paths() -> Dict[str, str]:
    return load_merge_paths()


def test_linear_path(merge_paths: Dict[str, str]):
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


def test_parent_branch_is_on_path(parent_branch: str, merge_paths: Dict[str, str]):
    known_branches = set(merge_paths.keys()) | set(merge_paths.values())
    assert parent_branch in known_branches, (
        f"Parent branch '{parent_branch}' is not on the merge path (branches in merge path: "
        f"{known_branches})."
    )
