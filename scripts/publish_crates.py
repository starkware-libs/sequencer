#!/usr/bin/env python3.9

import argparse
import asyncio
import json
import subprocess
import sys
from typing import List, Set

import toml


async def crate_version_exists(crate_name: str, version: str) -> bool:
    response = subprocess.run(
        ["curl", "-s", f"https://crates.io/api/v1/crates/{crate_name}"],
        capture_output=True,
        text=True,
    )

    crate_metadata = json.loads(response.stdout)
    already_published = version in [v["num"] for v in crate_metadata["versions"]]
    print(
        f"Crate {crate_name} version {version} "
        f"{'exists' if already_published else 'does not exist'} on crates.io"
    )
    return already_published


def get_workspace_version() -> str:
    try:
        cargo_data = toml.load("Cargo.toml")

        return cargo_data["workspace"]["package"]["version"]
    except (KeyError, TypeError):
        raise ValueError("Version key not found in Cargo.toml")


async def verify_unpublished(crates: List[str]):
    """
    Asserts that none of the crates in the set have been published.
    """

    version = get_workspace_version()

    tasks = [crate_version_exists(crate_name=crate, version=version) for crate in crates]
    results = await asyncio.gather(*tasks)

    already_published = [crate for crate, unpublished in zip(crates, results) if unpublished]

    assert (
        not already_published
    ), f"Crates {already_published} have already been published with {version=}."


def get_workspace_packages() -> List[dict]:
    """
    Returns the `cargo metadata` entries of the workspace members.
    """
    metadata = json.loads(
        subprocess.check_output(["cargo", "metadata", "--no-deps", "--format-version", "1"])
    )
    return metadata["packages"]


def get_package_and_dependencies_in_order(crate: str, packages: List[dict]) -> List[str]:
    """
    Returns the workspace members (from `packages`) that must be on crates.io before `crate` can be
    published, in topological order: if crate A depends on crate B, B appears before A. The input
    crate is the last element.

    `cargo publish` resolves every dependency the published manifest keeps against crates.io, so
    each one has to be published first. The manifest keeps every normal and build dependency,
    optional ones included, and every dev-dependency that has a version requirement. Cargo drops a
    dev-dependency that has only a path (`cargo metadata` reports its requirement as `*`), so those
    workspace-only test helpers stay out of the order.
    """
    packages_by_name = {package["name"]: package for package in packages}
    if crate not in packages_by_name:
        raise ValueError(f"{crate} is not a workspace member.")

    def required_member_dependencies(package_name: str) -> List[str]:
        required_names = set()
        for dependency in packages_by_name[package_name]["dependencies"]:
            if dependency["name"] not in packages_by_name:
                continue
            if dependency["kind"] == "dev" and dependency["req"] == "*":
                continue
            required_names.add(dependency["name"])
        return sorted(required_names)

    ordered_crates: List[str] = []
    # Crates on the current DFS path; meeting one again means a cycle, which cannot be published.
    crates_in_progress: List[str] = []

    def visit(package_name: str):
        if package_name in ordered_crates:
            return
        if package_name in crates_in_progress:
            cycle = crates_in_progress[crates_in_progress.index(package_name) :] + [package_name]
            cycle_description = " -> ".join(cycle)
            raise ValueError(f"Cannot order a dependency cycle for publishing: {cycle_description}")
        crates_in_progress.append(package_name)
        for dependency_name in required_member_dependencies(package_name):
            visit(dependency_name)
        crates_in_progress.pop()
        ordered_crates.append(package_name)

    visit(crate)
    return ordered_crates


def get_publishable_crates(packages: List[dict]) -> Set[str]:
    """
    Returns the workspace members that may be published to crates.io. `cargo metadata` reports
    `publish` as null when unrestricted, as an empty list for `publish = false`, and as the list of
    allowed registries otherwise.
    """
    return {
        package["name"]
        for package in packages
        if package["publish"] is None or "crates-io" in package["publish"]
    }


def get_unpublishable_crates(crates: List[str], packages: List[dict]) -> List[str]:
    """
    Returns the entries of `crates` that may not be published to crates.io, in their given order.
    """
    publishable_crates = get_publishable_crates(packages=packages)
    return [crate for crate in crates if crate not in publishable_crates]


async def publish_crate_and_dependencies(crate: str, dry_run: bool):
    packages = get_workspace_packages()
    dependencies = get_package_and_dependencies_in_order(crate=crate, packages=packages)
    assert crate == dependencies[-1], f"{crate} should be the last element of '{dependencies}'."

    unpublishable = get_unpublishable_crates(crates=dependencies, packages=packages)
    assert not unpublishable, (
        f"Crates {unpublishable} are marked `publish = false`. Drop the flag and give them a "
        f"version in the workspace manifest before publishing {crate}."
    )

    # Do not attempt to publish anything if even one of the dependencies is already published.
    await verify_unpublished(crates=dependencies)

    base_command_template = "cargo publish -p {crate}" + f"{' --dry-run' if dry_run else ''}"
    # Publish order is important.
    cmd = " && ".join(
        [base_command_template.format(crate=dependency) for dependency in dependencies]
    )

    print(f"Publishing {crate} ({dry_run=}) and its dependencies: {dependencies}...")
    print(cmd, flush=True)
    subprocess.run(cmd, check=True, shell=True)
    print(f"Done.")


async def main():
    parser = argparse.ArgumentParser(
        description="Publish a crate and it's dependencies in the local workspace."
    )
    parser.add_argument(
        "--crate",
        required=True,
        type=str,
        help="Crate to publish (dependencies will also be published).",
    )
    parser.add_argument("--dry_run", required=False, action="store_true", help="Dry run.")
    args = parser.parse_args()

    await publish_crate_and_dependencies(crate=args.crate, dry_run=args.dry_run)


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
