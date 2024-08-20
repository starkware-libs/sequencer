import argparse
import subprocess
from typing import List, Set


def verify_unpublished(crates: Set[str]):
    """
    Asserts that none of the crates in the set have been published.
    """
    raise NotImplementedError("Not implemented yet.")


def get_package_and_dependencies_in_order(crate: str) -> List[str]:
    """
    Returns a list of all crates that the input crate depends on, in topological order.
    I.e, if crate A depends on crate B, then B will appear before A in the list.
    The output list also includes the input crate (last element).
    """
    # Fetch all *local* dependencies of the input crate.
    # We use the `depth` prefix to easily sort the dependencies in topological order: higher depth
    # means the crate is depended on by the crate at the lower depth.
    prefixed_tree = subprocess.Popen(
        ["cargo", "tree", "-p", crate, "--prefix", "depth"], stdout=subprocess.PIPE
    )
    # Locality is ensured by grepping "(/home" in the output of `cargo tree`.
    prefixed_local_crate_lines = (
        subprocess.check_output(["grep", "(/home"], stdin=prefixed_tree.stdout)
        .decode()
        .splitlines()
    )
    # Clean up the lines to only keep the crate names with their depth prefix.
    prefixed_local_crates = [line.split()[0].strip() for line in prefixed_local_crate_lines]

    # Reverse order to iterate in ascending depth order.
    ordered_dependencies = []
    for dependency_with_depth in reversed(sorted(prefixed_local_crates)):
        # Strip the leading depth number (package names do not start with integers).
        dependency = dependency_with_depth.lstrip("0123456789")
        # The same package may appear multiple times, and with different depths. Always keep the
        # highest depth only.
        if dependency not in ordered_dependencies:
            ordered_dependencies.append(dependency)
    return ordered_dependencies


def publish_crate_and_dependencies(crate: str, dry_run: bool):
    dependencies = get_package_and_dependencies_in_order(package_name=crate)
    assert crate in dependencies, f"{crate} not found in its dependencies: {dependencies}."

    # Do not attempt to publish anything if even one of the dependencies is already published.
    verify_unpublished(crates=dependencies)

    base_command_template = "cargo publish -p {crate}" + f"{' --dry-run' if dry_run else ''}"
    # Publish order is important.
    cmd = " && ".join(
        [base_command_template.format(crate=dependency) for dependency in dependencies]
    )

    print(f"Publishing {crate} (dry_run is {dry_run}) and its dependencies: {dependencies}...")
    print(cmd, flush=True)
    subprocess.run(cmd, check=True)
    print(f"Done.")


def main():
    parser = argparse.ArgumentParser(
        description="Publish a crate and it's dependencies in the local workspace."
    )
    parser.add_argument(
        "--crate", type=str, help="Crate to publish (dependencies will also be published)."
    )
    parser.add_argument("--dry_run", required=False, action="store_true", help="Dry run.")
    args = parser.parse_args()

    publish_crate_and_dependencies(crate=args.crate, dry_run=args.dry_run)


if __name__ == "__main__":
    main()
