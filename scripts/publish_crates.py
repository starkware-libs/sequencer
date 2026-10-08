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


def get_package_and_dependencies_in_order(crate: str) -> List[str]:
    """
    Returns a list of all local (member) crates that the input crate depends on, in topological
    order. I.e, if crate A depends on crate B, then B will appear before A in the list.
    The output list also includes the input crate (last element).
    """
    # We use the `depth` prefix to easily sort the dependencies in topological order: higher depth
    # means the crate is depended on by the crate at the lower depth.
    # Dev-dependencies are not part of a published crate, so only follow normal and build edges;
    # otherwise workspace-only test helpers would end up in the publish list.
    prefixed_tree = (
        subprocess.check_output(
            ["cargo", "tree", "-p", crate, "--prefix", "depth", "-e", "normal,build"]
        )
        .decode()
        .splitlines()
    )
    # Clean up the lines to only keep the *local* crate names with their depth prefix.
    # Skip all non-local crates ('(/home' should appear in lines describing local crates).
    prefixed_local_crates = [line.split()[0].strip() for line in prefixed_tree if "(/home" in line]

    # Reverse order to iterate in descending depth order.
    ordered_dependencies = []
    for dependency_with_depth in reversed(sorted(prefixed_local_crates)):
        # Strip the leading depth number (package names do not start with integers).
        dependency = dependency_with_depth.lstrip("0123456789")
        # The same package may appear multiple times, and with different depths. Always keep the
        # highest depth only.
        if dependency not in ordered_dependencies:
            ordered_dependencies.append(dependency)
    return ordered_dependencies


def get_publishable_crates() -> Set[str]:
    """
    Returns the workspace members that may be published to crates.io: those whose manifest does
    not restrict `publish`. `cargo metadata` reports `publish` as null when unrestricted, as an
    empty list for `publish = false`, and as a list of registries otherwise.
    """
    metadata = json.loads(
        subprocess.check_output(["cargo", "metadata", "--no-deps", "--format-version", "1"])
    )
    return {package["name"] for package in metadata["packages"] if package["publish"] is None}


async def publish_crate_and_dependencies(crate: str, dry_run: bool):
    dependencies = get_package_and_dependencies_in_order(crate=crate)
    assert crate == dependencies[-1], f"{crate} should be the last element of '{dependencies}'."

    publishable_crates = get_publishable_crates()
    unpublishable = [
        dependency for dependency in dependencies if dependency not in publishable_crates
    ]
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
