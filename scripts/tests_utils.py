#!/bin/env python3

import os
import re
import subprocess
from typing import Dict, List, Optional, Set

from git import Repo

# Set of files which - if changed - should trigger tests for all packages.
ALL_TEST_TRIGGERS: Set[str] = {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml"}
PATTERN = r"(\w+)\s*v([\d.]*.*)\((.*?)\)"

# Pattern to match the dependency tree output (`cargo tree -i` output).
# First match group is the dependent crate name; second match group is the local path to the
# dependant crate.
# '([a-zA-Z0-9_]+)' is the crate name.
# ' [^(]* ' is anything between the crate name and the path (path is in parens).
# '\(([^)]+)\)' should match the path to the crate. No closing paren in the path.
DEPENDENCY_PATTERN = r"([a-zA-Z0-9_]+) [^(]* \(([^)]+)\)"


def get_workspace_tree() -> Dict[str, str]:
    tree = dict()
    res = subprocess.check_output("cargo tree --depth 0".split()).decode("utf-8").splitlines()
    for l in res:
        m = re.match(PATTERN, l)
        if m is not None:
            tree.update({m.group(1): m.group(3)})
    return tree


def get_workspace_packages() -> Set[str]:
    return set(get_workspace_tree().keys())


def get_local_changes(repo_path, commit_id: Optional[str]) -> List[str]:
    os.environ["GIT_PYTHON_REFRESH"] = "quiet"  # noqa
    repo = Repo(repo_path)
    try:
        repo.head.object  # Check if local_repo is a git repo.
    except ValueError:
        print(f"unable to validate {repo_path} as a git repo.")
        raise

    return [c.a_path for c in repo.head.commit.diff(commit_id)]


def get_modified_packages(files: List[str]) -> Set[str]:
    tree = get_workspace_tree()
    packages = set()
    for file in files:
        for p_name, p_path in tree.items():
            if os.path.abspath(file).startswith(p_path):
                packages.add(p_name)
    return packages


def get_package_dependencies(package_name: str) -> Set[str]:
    res = (
        subprocess.check_output(f"cargo tree -i {package_name} --prefix none".split())
        .decode("utf-8")
        .splitlines()
    )
    deps = set()
    for l in res:
        m = re.match(DEPENDENCY_PATTERN, l)
        if m is not None:
            deps.add(m.group(1))
    return deps


def packages_to_test_due_to_global_changes(files: List[str]) -> Set[str]:
    if len(set(files).intersection(ALL_TEST_TRIGGERS)) > 0:
        return set(get_workspace_tree().keys())
    return set()


def get_tested_packages(
    changes_only: bool,
    commit_id: Optional[str],
    include_dependencies: bool,
):
    """
    Get packages to be tested.
    If changes_only is True, only tests packages that have been modified; if no packages have been
    modified, no packages are returned. If changes_only is False, return all packages.
    If commit_id is provided, compares against that commit; otherwise, compares against HEAD.
    """
    if not changes_only:
        assert not include_dependencies, "include_dependencies can only be set with changes_only."
    tested_packages = set()
    if changes_only:
        local_changes = get_local_changes(".", commit_id=commit_id)
        modified_packages = get_modified_packages(local_changes)

        if include_dependencies:
            for p in modified_packages:
                deps = get_package_dependencies(p)
                tested_packages.update(deps)
            print(
                f"Running tests for {tested_packages} (due to modifications in "
                f"{modified_packages})."
            )
        else:
            print(f"Running tests for modified crates {modified_packages}.")
            tested_packages = modified_packages

        # Add global-triggered packages.
        extra_packages = packages_to_test_due_to_global_changes(files=local_changes)
        print(f"Running tests for global-triggered packages {extra_packages}")
        tested_packages.update(extra_packages)
    return tested_packages
