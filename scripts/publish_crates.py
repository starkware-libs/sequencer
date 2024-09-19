import argparse
import subprocess
from typing import List, Optional
import toml
import json


def crate_version_exists(crate_name: str, version: str) -> bool:

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


def verify_unpublished(crates: List[str], version: Optional[str] = None):
    """
    Asserts that none of the crates in the set have been published.
    """

    version = get_workspace_version()

    already_published = [
        crate for crate in crates if crate_version_exists(crate_name=crate, version=version)
    ]

    assert (
        not already_published
    ), f"Crates {already_published} have already been published with {version=}."


def get_package_and_dependencies_in_order(crates: List[str]) -> List[str]:
    """
    Returns a list of all local (member) crates that the input crates depends on, in topological
    order. I.e, if crate A depends on crate B, then B will appear before A in the list.
    The output list also includes the input crates.
    Each crate is only included once in the output list.
    """
    all_crates_dependencies: List[str] = []
    for crate in crates:
        # We use the `depth` prefix to easily sort the dependencies in topological order: higher
        # depth means the crate is depended on by the crate at the lower depth.
        prefixed_tree = (
            subprocess.check_output(["cargo", "tree", "-p", crate, "--prefix", "depth"])
            .decode()
            .splitlines()
        )
        # Clean up the lines to only keep the *local* crate names with their depth prefix.
        # Skip all non-local crates ('(/home' should appear in lines describing local crates).
        prefixed_local_crates = [
            line.split()[0].strip() for line in prefixed_tree if "(/home" in line
        ]

        # Reverse order to iterate in descending depth order.
        ordered_dependencies = []
        for dependency_with_depth in reversed(sorted(prefixed_local_crates)):
            # Strip the leading depth number (package names do not start with integers).
            dependency = dependency_with_depth.lstrip("0123456789")
            # The same package may appear multiple times, and with different depths. Always keep the
            # highest depth only.
            if dependency not in ordered_dependencies and dependency not in all_crates_dependencies:
                ordered_dependencies.append(dependency)
    return ordered_dependencies


def publish_crates_and_dependencies(crates: List[str], dry_run: bool):

    all_crates_dependencies = get_package_and_dependencies_in_order(crates=crates)

    # Do not attempt to publish anything if even one of the dependencies is already published.
    verify_unpublished(crates=all_crates_dependencies)

    base_command_template = "cargo publish -p {crate}" + f"{' --dry-run' if dry_run else ''}"
    # Publish order is important.
    cmd = " && ".join(
        [base_command_template.format(crate=dependency) for dependency in all_crates_dependencies]
    )

    print(f"Publishing crates: {all_crates_dependencies} ({dry_run=})...")
    print(cmd, flush=True)
    subprocess.run(cmd, check=True, shell=True)
    print(f"Done.")


def main():
    parser = argparse.ArgumentParser(
        description="Publish multiple crates and their dependencies in the local workspace."
    )
    parser.add_argument(
        "--crates",
        type=str,
        help="List of crates to publish (dependencies will also be published).",
        nargs="+",
    )
    parser.add_argument("--dry_run", required=False, action="store_true", help="Dry run.")
    args = parser.parse_args()

    publish_crates_and_dependencies(crates=args.crates, dry_run=args.dry_run)


if __name__ == "__main__":
    main()
