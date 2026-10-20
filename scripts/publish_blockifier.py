#!/usr/bin/env python3
"""
Publishes blockifier, and the workspace crates it depends on, to crates.io.

Run from a release branch: main carries the placeholder workspace version 0.0.0, which is never
published. Crates already published at the target version are skipped, so a run that failed
midway can be resumed by running it again.

Subcommands:
    status   Print the publish plan and what is already on crates.io, as JSON.
    bump     Set the workspace version (Cargo.toml and Cargo.lock) to a new version.
    publish  Publish the crates in dependency order and tag the release.
"""

import argparse
import json
import os
import re
import subprocess
import sys
import urllib.error
import urllib.request
from graphlib import CycleError, TopologicalSorter
from typing import Dict, List, Optional, Set

ROOT_CRATE = "blockifier"
TAG_PREFIX = "blockifier-v"
PLACEHOLDER_VERSION = "0.0.0"
CRATES_IO_API_URL = "https://crates.io/api/v1/crates"
# crates.io rejects API requests that carry no User-Agent.
CRATES_IO_USER_AGENT = "starkware-libs/sequencer publish_blockifier.py"
# Matches semver versions such as 0.20.0, 0.20.0-rc.1 (build metadata is not used for releases).
VERSION_PATTERN = re.compile(r"^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$")


def status_command(args: argparse.Namespace):
    workspace_version = get_workspace_version()
    publish_order = get_publish_order()
    published_versions = get_published_versions(crate=ROOT_CRATE)
    status = {
        "branch": get_current_branch(),
        "commit": git_output(["rev-parse", "HEAD"]),
        "workspace_version": workspace_version,
        "latest_published_version": get_latest_published_version(crate=ROOT_CRATE),
        "workspace_version_published": workspace_version in published_versions,
        "tag": f"{TAG_PREFIX}{workspace_version}",
        "tag_commit": get_remote_tag_commit(tag=f"{TAG_PREFIX}{workspace_version}"),
        "publish_order": [
            {
                "crate": crate,
                "published": workspace_version in get_published_versions(crate=crate),
            }
            for crate in publish_order
        ],
    }
    print(json.dumps(status, indent=2))


def bump_command(args: argparse.Namespace):
    new_version = args.version
    validate_version_format(version=new_version)
    old_version = get_workspace_version()
    if new_version == old_version:
        sys.exit(f"The workspace version is already {new_version}.")
    if old_version == PLACEHOLDER_VERSION:
        sys.exit("The workspace has the placeholder version; release versions live on branches.")
    if new_version in get_published_versions(crate=ROOT_CRATE):
        sys.exit(f"{ROOT_CRATE} {new_version} is already published on crates.io.")

    with open("Cargo.toml") as cargo_toml_file:
        cargo_toml = cargo_toml_file.read()
    old_version_field = f'version = "{old_version}"'
    new_version_field = f'version = "{new_version}"'
    bumped_lines = []
    num_bumped_lines = 0
    in_workspace_package = False
    for line in cargo_toml.splitlines(keepends=True):
        if line.startswith("["):
            in_workspace_package = line.strip() == "[workspace.package]"
        # Only bump the workspace version and the versions of path dependencies on workspace
        # crates; an external dependency may happen to share the same version number.
        if old_version_field in line and (in_workspace_package or "path = " in line):
            line = line.replace(old_version_field, new_version_field)
            num_bumped_lines += 1
        bumped_lines.append(line)
    with open("Cargo.toml", "w") as cargo_toml_file:
        cargo_toml_file.write("".join(bumped_lines))

    # Only re-resolves workspace members, so no external dependency is upgraded by the bump.
    subprocess.run(["cargo", "update", "--workspace"], check=True)
    print(f"Bumped {num_bumped_lines} versions in Cargo.toml: {old_version} -> {new_version}.")


def publish_command(args: argparse.Namespace):
    version = args.version
    validate_version_format(version=version)
    workspace_version = get_workspace_version()
    if version != workspace_version:
        sys.exit(f"Requested {version}, but the workspace version is {workspace_version}.")
    if version == PLACEHOLDER_VERSION:
        sys.exit(f"{PLACEHOLDER_VERSION} is a placeholder version and is never published.")
    if get_current_branch() == "main":
        sys.exit("Publishing from main is not allowed; publish from a release branch.")

    # Uploading is reserved for the publish workflow, whose crates.io token is only released after
    # an authorized reviewer approves the run; this keeps local or bot runs from publishing even
    # if credentials happen to be present.
    if not args.dry_run and os.environ.get("GITHUB_ACTIONS") != "true":
        sys.exit("Publishing is only done by the blockifier_publish.yml workflow; use --dry-run.")

    publish_order = get_publish_order()
    tag = f"{TAG_PREFIX}{version}"
    head_commit = git_output(["rev-parse", "HEAD"])
    tag_commit = get_remote_tag_commit(tag=tag)
    if tag_commit is not None and tag_commit != head_commit:
        sys.exit(f"Tag {tag} already points at {tag_commit}, not at HEAD ({head_commit}).")

    pending_crates = [
        crate for crate in publish_order if version not in get_published_versions(crate=crate)
    ]
    published_crates = [crate for crate in publish_order if crate not in pending_crates]
    print(f"Publish order: {publish_order}")
    print(f"Already published at {version}: {published_crates}")
    print(f"To publish: {pending_crates}", flush=True)

    if args.dry_run:
        if pending_crates:
            # A single multi-package invocation resolves the not-yet-published crates against
            # each other, so dependents can be verified before their dependencies are on
            # crates.io.
            run_cargo_publish(crates=pending_crates, dry_run=True)
        print("Dry run passed.")
        return

    # Tag as soon as the first crate is out: from then on the version is taken on crates.io, and
    # the tag records which commit it was published from.
    is_tagged = tag_commit is not None
    if published_crates and not is_tagged:
        push_tag(tag=tag)
        is_tagged = True
    for crate in pending_crates:
        run_cargo_publish(crates=[crate], dry_run=False)
        if not is_tagged:
            push_tag(tag=tag)
            is_tagged = True
    print(f"Published {ROOT_CRATE} {version} and its dependencies; tagged {tag}.")


def get_publish_order() -> List[str]:
    """
    Returns the workspace crates that must be published for blockifier to be published, ordered
    such that every crate comes after the workspace crates it depends on. The last element is
    blockifier.
    """
    metadata = json.loads(
        subprocess.check_output(["cargo", "metadata", "--format-version", "1", "--no-deps"])
    )
    workspace_packages = {
        package["name"]: package
        for package in metadata["packages"]
        if package["id"] in metadata["workspace_members"]
    }

    dependency_graph: Dict[str, Set[str]] = {}
    crates_to_visit = [ROOT_CRATE]
    while crates_to_visit:
        crate = crates_to_visit.pop()
        if crate in dependency_graph:
            continue
        dependency_graph[crate] = set()
        for dependency in workspace_packages[crate]["dependencies"]:
            if dependency["name"] not in workspace_packages or dependency.get("path") is None:
                continue
            # cargo strips dev-dependencies that have no version when publishing; all other
            # workspace dependencies must exist on crates.io first.
            if dependency["kind"] == "dev" and dependency["req"] == "*":
                continue
            dependency_graph[crate].add(dependency["name"])
            crates_to_visit.append(dependency["name"])

    try:
        publish_order = list(TopologicalSorter(dependency_graph).static_order())
    except CycleError as error:
        sys.exit(f"Workspace dependencies of {ROOT_CRATE} form a cycle: {error.args[1]}")
    assert publish_order[-1] == ROOT_CRATE, f"{ROOT_CRATE} must be last in {publish_order}."
    return publish_order


def get_workspace_version() -> str:
    """Returns the version of blockifier, which inherits the workspace version."""
    metadata = json.loads(
        subprocess.check_output(["cargo", "metadata", "--format-version", "1", "--no-deps"])
    )
    (version,) = {
        package["version"] for package in metadata["packages"] if package["name"] == ROOT_CRATE
    }
    return version


def get_published_versions(crate: str) -> Set[str]:
    """Returns all versions of the crate on crates.io, including yanked ones (which can't be
    republished either). Returns an empty set if the crate was never published."""
    crate_metadata = fetch_crate_metadata(crate=crate)
    if crate_metadata is None:
        return set()
    return {version["num"] for version in crate_metadata["versions"]}


def get_latest_published_version(crate: str) -> Optional[str]:
    """Returns the highest version on crates.io (including pre-releases, which is what most
    blockifier releases are), or None if the crate was never published."""
    crate_metadata = fetch_crate_metadata(crate=crate)
    return None if crate_metadata is None else crate_metadata["crate"]["max_version"]


def fetch_crate_metadata(crate: str) -> Optional[dict]:
    """Returns the crates.io API response for the crate, or None if it does not exist."""
    request = urllib.request.Request(
        f"{CRATES_IO_API_URL}/{crate}", headers={"User-Agent": CRATES_IO_USER_AGENT}
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.load(response)
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return None
        raise


def run_cargo_publish(crates: List[str], dry_run: bool):
    command = ["cargo", "publish"]
    for crate in crates:
        command += ["-p", crate]
    if dry_run:
        command.append("--dry-run")
    print(" ".join(command), flush=True)
    subprocess.run(command, check=True)


def push_tag(tag: str):
    subprocess.run(["git", "tag", tag], check=True)
    subprocess.run(["git", "push", "origin", f"refs/tags/{tag}"], check=True)


def get_remote_tag_commit(tag: str) -> Optional[str]:
    """Returns the commit the tag points at on origin, or None if the tag does not exist."""
    commits_by_ref = {}
    for ls_remote_line in git_output(["ls-remote", "--tags", "origin", tag]).splitlines():
        commit, ref = ls_remote_line.split("\t")
        commits_by_ref[ref] = commit
    # For an annotated tag, the `^{}` entry holds the commit (the plain entry is the tag object).
    return commits_by_ref.get(f"refs/tags/{tag}^{{}}", commits_by_ref.get(f"refs/tags/{tag}"))


def get_current_branch() -> str:
    """Returns the checked-out branch name, or an empty string on a detached HEAD."""
    return git_output(["branch", "--show-current"])


def git_output(git_args: List[str]) -> str:
    return subprocess.check_output(["git"] + git_args).decode().strip()


def validate_version_format(version: str):
    if not VERSION_PATTERN.match(version):
        sys.exit(f"Invalid version {version!r}; expected e.g. 0.20.0 or 0.20.0-rc.2.")


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0].strip())
    subparsers = parser.add_subparsers(dest="command", required=True)

    status_parser = subparsers.add_parser("status", help="Print the publish plan as JSON.")
    status_parser.set_defaults(handler=status_command)

    bump_parser = subparsers.add_parser("bump", help="Set a new workspace version.")
    bump_parser.add_argument("--version", required=True, help="New version, e.g. 0.20.0-rc.2.")
    bump_parser.set_defaults(handler=bump_command)

    publish_parser = subparsers.add_parser("publish", help="Publish blockifier and dependencies.")
    publish_parser.add_argument(
        "--version",
        required=True,
        help="Version to publish; must equal the workspace version.",
    )
    publish_parser.add_argument(
        "--dry-run", action="store_true", help="Package and verify without uploading."
    )
    publish_parser.set_defaults(handler=publish_command)

    args = parser.parse_args()
    args.handler(args)


if __name__ == "__main__":
    main()
