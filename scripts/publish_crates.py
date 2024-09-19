import argparse
import subprocess
from typing import List, Optional
import toml


def check_crate_version_exists(crate_name: str, version: str) -> bool:

    response = subprocess.run(
        ["curl", "-s", f"https://crates.io/api/v1/crates/{crate_name}"],
        capture_output=True,
        text=True,
    )

    if version in response.stdout:
        print(f"Crate {crate_name} version {version} exists on crates.io")
        return True
    else:
        print(f"Crate {crate_name} version {version} does not exist on crates.io")
        return False


def get_workspace_version(cargo_toml_path) -> str:
    try:
        cargo_data = toml.load(cargo_toml_path)

        return cargo_data["workspace"]["package"]["version"]
    except (KeyError, TypeError):
        raise ValueError("Version key not found in Cargo.toml")


def verify_unpublished(crates: List[str], version: Optional[str] = None):
    """
    Asserts that none of the crates in the set have been published.
    """
    if not version:
        version = get_workspace_version("Cargo.toml")
    for crate in crates:
        assert not check_crate_version_exists(crate_name=crate, version=version)


def get_package_and_dependencies_in_order(
    crate: str, all_crates_dependencies: List[str]
) -> List[str]:
    """
    Returns a list of all local (member) crates that the input crates depends on, in topological
    order. I.e, if crate A depends on crate B, then B will appear before A in the list.
    The output list also includes the input crates.
    Each crate is only included once in the output list.
    """
    # We use the `depth` prefix to easily sort the dependencies in topological order: higher depth
    # means the crate is depended on by the crate at the lower depth.
    prefixed_tree = (
        subprocess.check_output(["cargo", "tree", "-p", crate, "--prefix", "depth"])
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
        if dependency not in ordered_dependencies and dependency not in all_crates_dependencies:
            ordered_dependencies.append(dependency)
    return ordered_dependencies


def publish_crates_and_dependencies(crates: List[str], dry_run: bool, skip_version_check: bool):

    all_crates_dependencies: List[str] = []
    for crate in crates:
        all_crates_dependencies.extend(
            get_package_and_dependencies_in_order(
                crate=crate, all_crates_dependencies=all_crates_dependencies
            )
        )

    if not skip_version_check:
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
    parser.add_argument(
        "--skip_version_check", required=False, action="store_true", help="Skip version check."
    )
    args = parser.parse_args()

    publish_crates_and_dependencies(
        crates=args.crates, dry_run=args.dry_run, skip_version_check=args.skip_version_check
    )


if __name__ == "__main__":
    main()
