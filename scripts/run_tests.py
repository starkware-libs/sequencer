#!/bin/env python3
from ast import arg
import re
import subprocess
import os
from typing import Dict, List, Set
from git import Repo

PATTERN = r"(\w+)\s*v([\d.]*.*)\((.*?)\)"

def get_workspace_tree() -> Dict[str, str]:
    tree = dict()
    res = (
        subprocess.check_output("cargo tree --depth 0".split())
        .decode("utf-8")
        .splitlines()
    )
    for l in res:
        m = re.match(PATTERN, l)
        if m is not None:
            tree.update({m.group(1): m.group(3)})
    return tree


def get_local_changes(repo_path) -> List[str]:
    os.environ["GIT_PYTHON_REFRESH"] = "quiet"  # noqa
    repo = Repo(repo_path)
    try:
        repo.head.object  # Check if local_repo is a git repo.
    except ValueError:
        print(f"unable to validate {repo_path} as a git repo.")
        raise

    return [c.a_path for c in repo.head.commit.diff(None)]


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
        subprocess.check_output(f"cargo tree -i {package_name}".split())
        .decode("utf-8")
        .splitlines()
    )
    deps = set()
    for l in res:
        m = re.match(PATTERN, l)
        if m is not None:
            deps.add(m.group(1))
    return deps


def run_test():
    local_changes = get_local_changes(".")
    modified_packages = get_modified_packages(local_changes)
    for p in modified_packages:
        deps = get_package_dependencies(p)
        print(f"Running tests for {deps}")
        args = []
        for d in deps:
            args.extend(["--package", d])
    cmd = ["cargo", "test"] + args
    print(cmd)
    subprocess.run(cmd)

if __name__ == "__main__":
    print("Running tests...")
    run_test()
    # Run tests here
    print("Tests complete.")
