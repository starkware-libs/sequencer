#!/bin/env python3
import argparse
from typing import Dict, List

from utils import run_command

# Usage:
# scripts/generate_changelog.py --start <FROM_TAG> --end <TO_TAG> --project <PROJECT_NAME>
GIT_CLIFF_VERSION = "2.9.0"
PROJECT_TO_PATHS: Dict[str, List[str]] = {"blockifier": ["crates/blockifier/"], "all": []}


def prepare_git_cliff(version: str) -> None:
    """
    Install git-cliff if missing.
    """
    run_command(
        command=(
            f'cargo install --list | grep -q "git-cliff v{version}" || '
            f"cargo install git-cliff@{version}"
        )
    )


def build_command(project_name: str, start_tag: str, end_tag: str) -> str:
    paths = PROJECT_TO_PATHS[project_name]
    include_paths = " ".join((f"--include-path {path}" for path in paths))
    return (
        f"git-cliff {start_tag}..{end_tag} -o changelog_{start_tag}_{end_tag}.md "
        f'--ignore-tags ".*-dev.[0-9]+" --tag {end_tag} '
        f"--config scripts/git-cliff.toml {include_paths}"
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Generate a changelog file for a given project.")
    parser.add_argument(
        "--start", type=str, help="The commit/tag that changelog's history starts from."
    )
    parser.add_argument("--end", type=str, help="The commit/tag that changelog's history ends at.")
    parser.add_argument(
        "--project",
        choices=PROJECT_TO_PATHS.keys(),
        help="The project that the changelog is generated for.",
    )
    args = parser.parse_args()
    prepare_git_cliff(version=GIT_CLIFF_VERSION)
    command = build_command(project_name=args.project, start_tag=args.start, end_tag=args.end)
    run_command(command=command)
