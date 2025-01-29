#!/bin/env python3

from calendar import c
import re
import subprocess
import os
from typing import Dict, List, Set, Optional
from git import Repo

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
