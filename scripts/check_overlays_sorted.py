#!/usr/bin/env python3.9

"""
Checks that all sequencerConfig keys in YAML overlay files are alphabetically sorted.
Use --fix to sort them in-place.
"""

import argparse
import glob
import os
import sys
from typing import List, Optional, Tuple

CURRENT_DIR = os.path.dirname(__file__)
ROOT_PROJECT_DIR = os.path.abspath(os.path.join(CURRENT_DIR, ".."))
OVERLAYS_DIR = os.path.join(ROOT_PROJECT_DIR, "deployments/sequencer/configs/overlays")


def _find_sequencer_config_keys(lines: List[str]) -> Optional[Tuple[int, List[int]]]:
    """
    Returns (seq_indent, key_line_indices) if a sequencerConfig section is found,
    or None otherwise.
    """
    seq_idx = None
    seq_indent = 0
    for i, line in enumerate(lines):
        stripped = line.lstrip()
        if stripped.startswith("sequencerConfig:"):
            seq_idx = i
            seq_indent = len(line) - len(stripped)
            break

    if seq_idx is None:
        return None

    key_line_indices = []
    for i in range(seq_idx + 1, len(lines)):
        line = lines[i]
        if not line.strip():
            continue
        indent = len(line) - len(line.lstrip())
        if indent > seq_indent:
            key_line_indices.append(i)
        else:
            break

    if not key_line_indices:
        return None

    return seq_indent, key_line_indices


def _sort_key(line: str) -> str:
    stripped = line.lstrip()
    if ": " in stripped:
        return stripped.split(": ", 1)[0]
    return stripped.rstrip(":").rstrip()


def check_file(filepath: str) -> List[str]:
    """Returns a list of error messages if sequencerConfig keys are out of order."""
    with open(filepath) as f:
        lines = f.readlines()

    result = _find_sequencer_config_keys(lines)
    if result is None:
        return []

    _, key_line_indices = result
    key_lines = [lines[i] for i in key_line_indices]

    errors = []
    for i in range(1, len(key_lines)):
        if _sort_key(key_lines[i]) < _sort_key(key_lines[i - 1]):
            errors.append(
                f"  '{_sort_key(key_lines[i])}' is out of order"
                f" (appears after '{_sort_key(key_lines[i - 1])}')"
            )

    return errors


def fix_file(filepath: str) -> bool:
    """Sort sequencerConfig keys alphabetically in-place. Returns True if the file changed."""
    with open(filepath) as f:
        lines = f.readlines()

    result = _find_sequencer_config_keys(lines)
    if result is None:
        return False

    _, key_line_indices = result
    key_lines = [lines[i] for i in key_line_indices]
    sorted_lines = sorted(key_lines, key=_sort_key)

    if sorted_lines == key_lines:
        return False

    new_lines = list(lines)
    for idx, new_line in zip(key_line_indices, sorted_lines):
        new_lines[idx] = new_line

    with open(filepath, "w") as f:
        f.writelines(new_lines)

    return True


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--fix", action="store_true", help="Sort keys in-place instead of just checking."
    )
    args = parser.parse_args()

    pattern = os.path.join(OVERLAYS_DIR, "**", "*.yaml")
    files = sorted(glob.glob(pattern, recursive=True))

    if args.fix:
        fixed = []
        for filepath in files:
            if fix_file(filepath):
                fixed.append(os.path.relpath(filepath, ROOT_PROJECT_DIR))
        if fixed:
            print(f"Sorted sequencerConfig keys in {len(fixed)} file(s):")
            for path in fixed:
                print(f"  {path}")
        else:
            print("All overlay sequencerConfig keys were already sorted.")
    else:
        violations: List[str] = []
        for filepath in files:
            errors = check_file(filepath)
            if errors:
                rel_path = os.path.relpath(filepath, ROOT_PROJECT_DIR)
                violations.append(f"{rel_path}:\n" + "\n".join(errors))

        if violations:
            print(
                "The following overlay files have sequencerConfig keys that are not"
                " alphabetically sorted:\n"
            )
            print("\n\n".join(violations))
            sys.exit(1)

        print("All overlay sequencerConfig keys are alphabetically sorted.")


if __name__ == "__main__":
    main()
