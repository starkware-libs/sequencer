#!/usr/bin/env python3.9

import argparse
import os
import re
import subprocess
from pathlib import Path
from typing import Dict, List, Optional, Set, Tuple

try:
    import tomllib  # Python 3.11+
except ImportError:
    try:
        import tomli as tomllib  # Python < 3.11, use tomli package
    except ImportError:
        # Fallback: parse manually
        tomllib = None

from merge_branches import FINAL_BRANCH, MERGE_PATHS_FILE, load_merge_paths
from tests_utils import get_local_changes
from utils import git_files

CURRENT_DIR = os.path.dirname(__file__)
ROOT_PROJECT_DIR = os.path.abspath(os.path.join(CURRENT_DIR, ".."))
PARENT_BRANCH = open(os.path.join(CURRENT_DIR, "parent_branch.txt")).read().strip()

# Default Python version for black formatting
DEFAULT_PY_VERSION = "py37"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run Python code style checks.")
    parser.add_argument(
        "--fix", action="store_true", help="Fix mode. Runs only fixable checks, in fix mode."
    )
    parser.add_argument(
        "--commit_id",
        type=str,
        help="Git commit ID to compare against. If provided, only checks changed files.",
    )
    return parser.parse_args()


def get_python_version_from_pyproject(pyproject_path: Path) -> Optional[str]:
    """Extract Python target version from pyproject.toml."""
    if tomllib is None:
        # Fallback: simple regex parsing for TOML
        try:
            with open(pyproject_path, "r", encoding="utf-8") as f:
                content = f.read()
                # Look for target-version = ['py310', 'py311', ...]
                match = re.search(r"target-version\s*=\s*\[(.*?)\]", content, re.DOTALL)
                if match:
                    versions_str = match.group(1)
                    versions = re.findall(r"['\"]py(\d+)['\"]", versions_str)
                    if versions:
                        # Get highest version
                        max_version = max(int(v) for v in versions)
                        return f"py{max_version}"
        except (FileNotFoundError, ValueError):
            pass
        return None

    try:
        with open(pyproject_path, "rb") as f:
            config = tomllib.load(f)

        # Check for black target-version
        black_config = config.get("tool", {}).get("black", {})
        target_versions = black_config.get("target-version", [])

        if target_versions:
            # Use the highest version (most permissive)
            # Convert py310 -> py310, py311 -> py311, etc.
            versions = [v for v in target_versions if isinstance(v, str) and v.startswith("py")]
            if versions:
                # Sort and get the highest (e.g., py312 > py311 > py310)
                versions.sort(reverse=True)
                return versions[0]
    except (FileNotFoundError, KeyError, ValueError):
        # File doesn't exist or invalid format
        pass
    return None


def find_project_root(file_path: str) -> Optional[str]:
    """
    Automatically detect project root by walking up the directory tree
    and looking for pyproject.toml files.

    Returns the relative path to the project root (e.g., "deployments/sequencer")
    or None if no pyproject.toml is found.
    """
    file_full_path = Path(ROOT_PROJECT_DIR) / file_path
    current_dir = file_full_path.parent.resolve()
    root_path = Path(ROOT_PROJECT_DIR).resolve()

    # Walk up the directory tree
    while current_dir != root_path.parent:
        pyproject_path = current_dir / "pyproject.toml"
        if pyproject_path.exists():
            # Found a pyproject.toml - this is a project root
            try:
                # Get relative path from repo root
                relative_path = current_dir.relative_to(root_path)
                return str(relative_path).replace("\\", "/")
            except ValueError:
                # current_dir is not under root_path, shouldn't happen but handle it
                break

        # Move up one directory
        parent = current_dir.parent
        if parent == current_dir:
            # Reached filesystem root
            break
        current_dir = parent

    # No pyproject.toml found - return None
    # Files without a project root will use default Python version (py37)
    return None


def group_files_by_project(files: List[str]) -> Dict[str, List[str]]:
    """Group files by their project root."""
    grouped: Dict[str, List[str]] = {}
    for file_path in files:
        project_root = find_project_root(file_path)
        if project_root:
            if project_root not in grouped:
                grouped[project_root] = []
            grouped[project_root].append(file_path)
        else:
            # Files not in known project roots go to "other"
            if "other" not in grouped:
                grouped["other"] = []
            grouped["other"].append(file_path)
    return grouped


def get_python_version_for_project(project_root: str) -> str:
    """Get Python version for a project, checking pyproject.toml if available."""
    pyproject_path = Path(ROOT_PROJECT_DIR) / project_root / "pyproject.toml"
    version = get_python_version_from_pyproject(pyproject_path)
    return version if version else DEFAULT_PY_VERSION


def get_changed_python_files(commit_id: Optional[str]) -> List[str]:
    """Get list of changed Python files."""
    if commit_id:
        # Get changed files compared to commit_id
        changed_files = get_local_changes(".", commit_id=commit_id)
        return [f for f in changed_files if f.endswith(".py")]
    else:
        # No commit_id provided, check all Python files
        return git_files("py")


def run_black_for_project(project_root: str, files: List[str], py_version: str, fix: bool):
    """Run black on files for a specific project."""
    if not files:
        return

    print(f"Running black (target: {py_version}) on {len(files)} files in {project_root}/")

    # Build full paths to files
    files_to_check = []
    for file_path in files:
        file_full_path = os.path.join(ROOT_PROJECT_DIR, file_path)
        if os.path.isfile(file_full_path):
            files_to_check.append(file_full_path)

    if not files_to_check:
        return

    # Run black on the specific files
    command = ["black", "-l", "100", "-t", py_version]
    if not fix:
        command += ["--check", "--diff", "--color"]

    command.extend(files_to_check)
    subprocess.check_output(command)


def run_isort_for_project(project_root: str, files: List[str], fix: bool):
    """Run isort on files for a specific project."""
    if not files:
        return

    print(f"Running isort on {len(files)} files in {project_root}/")

    # Build full paths to files
    files_to_check = []
    for file_path in files:
        file_full_path = os.path.join(ROOT_PROJECT_DIR, file_path)
        if os.path.isfile(file_full_path):
            files_to_check.append(file_full_path)

    if not files_to_check:
        return

    command = ["isort", "--settings-path", ROOT_PROJECT_DIR]
    if not fix:
        command.append("-c")

    # Run on specific files
    command.extend(files_to_check)
    subprocess.check_output(command)


def run_black(fix: bool, commit_id: Optional[str] = None):
    """Run black with project-aware Python version detection."""
    python_files = get_changed_python_files(commit_id)

    if not python_files:
        print("No Python files to check.")
        return

    # Group files by project
    grouped = group_files_by_project(python_files)

    if len(grouped) > 1:
        print(
            f"Detected {len(grouped)} projects with changes. Each will be checked with its own Python version:"
        )
        for project_root, files in grouped.items():
            py_version = get_python_version_for_project(project_root)
            print(f"  - {project_root}/ ({len(files)} files, Python {py_version})")

    # Run black for each project with appropriate Python version
    # Each project is checked independently - no conflicts!
    for project_root, files in grouped.items():
        py_version = get_python_version_for_project(project_root)
        run_black_for_project(project_root, files, py_version, fix)


def run_isort(fix: bool, commit_id: Optional[str] = None):
    """Run isort with project-aware settings."""
    python_files = get_changed_python_files(commit_id)

    if not python_files:
        print("No Python files to check.")
        return

    # Group files by project
    grouped = group_files_by_project(python_files)

    # Run isort for each project
    # Each project is checked independently with its own settings
    for project_root, files in grouped.items():
        run_isort_for_project(project_root, files, fix)


def verify_linear_path():
    """
    Verify the merge paths JSON describes a linear merge path.
    """
    merge_paths = load_merge_paths()
    src_dst_iter = iter(merge_paths.items())
    (oldest_branch, prev_dst_branch) = next(src_dst_iter)
    assert (
        oldest_branch not in merge_paths.values()
    ), f"Oldest branch '{oldest_branch}' cannot be a destination branch."

    for src_branch, dst_branch in src_dst_iter:
        assert prev_dst_branch == src_branch, (
            f"Since the merge graph is linear, the source branch '{src_branch}' must be the same "
            f"as the previous destination branch, which is '{prev_dst_branch}'. Check out "
            f"{MERGE_PATHS_FILE}."
        )
        prev_dst_branch = dst_branch

    assert (
        prev_dst_branch == FINAL_BRANCH
    ), f"The last destination is '{prev_dst_branch}' but must be '{FINAL_BRANCH}'."


def verify_parent_branch_is_on_path():
    merge_paths = load_merge_paths()
    known_branches = set(merge_paths.keys()) | set(merge_paths.values())
    assert PARENT_BRANCH in known_branches, (
        f"Parent branch '{PARENT_BRANCH}' is not on the merge path (branches in merge path: "
        f"{known_branches})."
    )


def merge_branches_checks():
    verify_linear_path()
    verify_parent_branch_is_on_path()


def run_autoflake(fix: bool, commit_id: Optional[str] = None):
    """Run autoflake on changed Python files or all files."""
    if commit_id:
        # Only check changed files
        files = get_changed_python_files(commit_id)
    else:
        # Check all files
        files = git_files("py")

    if not files:
        print("No Python files to check.")
        return

    flavor = "--in-place" if fix else "--check-diff"
    command = [
        "autoflake",
        "--remove-all-unused-imports",
        "--remove-unused-variables",
        "--ignore-init-module-imports",
        "--recursive",
        flavor,
        *files,
    ]
    try:
        subprocess.check_output(command)
    except subprocess.CalledProcessError as error:
        print(f"Autoflake found issues:\n{error.output.decode()}")
        raise error


def main():
    args = parse_args()
    commit_id = args.commit_id

    # If commit_id is provided, we're in PR mode - only check changed files
    # Otherwise, check all files (push mode)
    if commit_id:
        print(f"PR mode: Checking changed files compared to {commit_id}")
    else:
        print("Push mode: Checking all files")

    run_autoflake(fix=args.fix, commit_id=commit_id)
    run_black(fix=args.fix, commit_id=commit_id)
    run_isort(fix=args.fix, commit_id=commit_id)
    if not args.fix:
        # Unfixable checks.
        merge_branches_checks()


if __name__ == "__main__":
    main()
