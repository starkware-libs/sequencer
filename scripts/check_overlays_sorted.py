#!/usr/bin/env python3.9

"""
Checks that all sequencerConfig keys in YAML overlay files are alphabetically sorted.
Use --fix to sort them in-place (requires yq).
"""

import argparse
import glob
import os
import subprocess
import sys
from typing import List

import yaml

CURRENT_DIR = os.path.dirname(__file__)
ROOT_PROJECT_DIR = os.path.abspath(os.path.join(CURRENT_DIR, ".."))
OVERLAYS_DIR = os.path.join(ROOT_PROJECT_DIR, "deployments/sequencer/configs/overlays")
SEQUENCER_CONFIG_KEY = "sequencerConfig"


def _get_sequencer_config_keys(filepath: str) -> List[str]:
    """Returns the sequencerConfig keys in their current order, or [] if absent."""
    with open(filepath) as f:
        data = yaml.safe_load(f)
    try:
        sequencer_config = data["config"][SEQUENCER_CONFIG_KEY]
        return list(sequencer_config.keys())
    except (KeyError, TypeError):
        return []


def check_file(filepath: str) -> List[str]:
    """Returns a list of error messages if sequencerConfig keys are out of order."""
    keys = _get_sequencer_config_keys(filepath)
    errors = []
    for i in range(1, len(keys)):
        if keys[i] < keys[i - 1]:
            errors.append(f"  '{keys[i]}' is out of order (appears after '{keys[i - 1]}')")
    return errors


def fix_file(filepath: str) -> None:
    """Sort sequencerConfig keys alphabetically in-place using yq."""
    try:
        subprocess.run(
            ["yq", "-i", f".config.{SEQUENCER_CONFIG_KEY} |= sort_keys(.)", filepath],
            check=True,
        )
    except FileNotFoundError:
        print("Error: 'yq' is not installed. Install it with:")
        print(
            "  wget -qO ~/.local/bin/yq https://github.com/mikefarah/yq/releases/latest/download/yq_linux_amd64 && chmod +x ~/.local/bin/yq"
        )
        print("  # or: brew install yq")
        sys.exit(1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--fix",
        action="store_true",
        help="Sort keys in-place instead of just checking (requires yq).",
    )
    args = parser.parse_args()

    pattern = os.path.join(OVERLAYS_DIR, "**", "*.yaml")
    files = sorted(glob.glob(pattern, recursive=True))

    if args.fix:
        for filepath in files:
            if check_file(filepath):
                fix_file(filepath)
                print(f"  Fixed: {os.path.relpath(filepath, ROOT_PROJECT_DIR)}")
    else:
        violations: List[str] = []
        for filepath in files:
            errors = check_file(filepath)
            if errors:
                rel_path = os.path.relpath(filepath, ROOT_PROJECT_DIR)
                violations.append(f"{rel_path}:\n" + "\n".join(errors))

        if violations:
            print(
                f"The following overlay files have {SEQUENCER_CONFIG_KEY} keys that are not"
                " alphabetically sorted:\n"
            )
            print("\n\n".join(violations))
            sys.exit(1)

        print(f"All overlay {SEQUENCER_CONFIG_KEY} keys are alphabetically sorted.")


if __name__ == "__main__":
    main()
