#!/bin/env python3
import argparse
import itertools

from merge_branches import run_command

# Usage:
# scripts/generate_changelog.py --start <FROM_TAG> --end <TO_TAG> --project <PROJECT_NAME>


PROJECT_TO_CRATES = {
    "blockifier": ["blockifier", "native_blockifier"],
    "mempool": [
        "gateway",
        "mempool",
        "mempool_infra",
        "mempool_node",
        "mempool_test_utils",
        "mempool_types",
    ],
    "papyrus": [
        "papyrus_base_layer",
        "papyrus_common",
        "papyrus_config",
        "papyrus_execution",
        "papyrus_load_test",
        "papyrus_monitoring_gateway",
        "papyrus_network",
        "papyrus_node",
        "papyrus_p2p_sync",
        "papyrus_proc_macros",
        "papyrus_protobuf",
        "papyrus_rpc",
        "papyrus_storage",
        "papyrus_sync",
        "papyrus_test_utils",
        "sequencing",
    ],
    "committer": ["committer", "committer_cli", "starknet_committer"],
    "starknet_api": ["starknet_api"],
}

PROJECT_NAMES = PROJECT_TO_CRATES.keys()
CRATES = list(itertools.chain(*PROJECT_TO_CRATES.values()))
GIT_CLIFF_VERSION = "2.4.0"


def install_git_cliff(version: str):
    """
    Install git-cliff if missing.
    """
    run_command(
        command=f'cargo install --list | grep -q "git-cliff v{version}" || cargo install git-cliff@{version}'
    )


def build_command(project_name: str, start_tag: str, end_tag) -> str:
    if project_name not in PROJECT_NAMES:
        print(f"Invalid project name was given, given: {project_name} is not in {PROJECT_NAMES}")
        exit(1)
    crates = PROJECT_TO_CRATES[project_name]
    include_paths = "".join((f"--include-path crates/{crate}/ " for crate in crates))
    return (
        f'git-cliff {start_tag}..{end_tag} -o changelog_{start_tag}_{end_tag}.md --ignore-tags ".*-dev.[0-9]+" '
        + include_paths
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Generate a changelog file for a given project.")
    parser.add_argument(
        "--start", type=str, help="The commit that changelog's history starts from."
    )
    parser.add_argument("--end", type=str, help="The commit that changelog's history ends at.")
    parser.add_argument(
        "--project", type=str, help="The project that the changelog is generated for."
    )
    args = parser.parse_args()
    install_git_cliff(version=GIT_CLIFF_VERSION)
    command = build_command(project_name=args.project, start_tag=args.start, end_tag=args.end)
    run_command(command=command)
