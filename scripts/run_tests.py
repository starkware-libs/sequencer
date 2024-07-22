#!/bin/env python3

import argparse
from calendar import c
import re
import subprocess
import os
from typing import Dict, List, Set, Optional
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


def get_local_changes(repo_path, commit_id: Optional[str]) -> List[str]:
    os.environ["GIT_PYTHON_REFRESH"] = "quiet"  # noqa
    repo = Repo(repo_path)
    try:
        repo.head.object  # Check if local_repo is a git repo.
    except ValueError:
        print(f"unable to validate {repo_path} as a git repo.")
        raise

    print(f"{commit_id=}")
    print([c.a_path for c in repo.head.commit.diff(commit_id)])
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


def run_test(changes_only: bool, commit_id: Optional[str], features: Optional[str] = None):
    local_changes = get_local_changes(".", commit_id=commit_id)
    modified_packages = get_modified_packages(local_changes)
    args = []
    if changes_only:
        for p in modified_packages:
            deps = get_package_dependencies(p)
            print(f"Running tests for {deps}")
            for d in deps:
                args.extend(["--package", d])
        if len(args) == 0:
            print("No changes detected.")
            return
    cmd = ["cargo", "test"] + args

    if features is not None:
        cmd.extend(["--features", features])

    print("Running tests...")
    print(cmd)
    subprocess.run(cmd, check=True)
    print("Tests complete.")

def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Presubmit script.")
    parser.add_argument("--changes_only", action="store_true")
    parser.add_argument(
        "--features", type=str, help="Which services to deploy. For multi services separate by ','."
    )
    parser.add_argument(
        "--commit_id", type=str, help="GIT commit ID to compare against."
    )
    return parser.parse_args()


def main():
    args = parse_args()
    run_test(changes_only=args.changes_only, commit_id=args.commit_id, features=args.features)


if __name__ == "__main__":
    main()
