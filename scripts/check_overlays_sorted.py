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

CURRENT_DIR = os.path.dirname(__file__)
ROOT_PROJECT_DIR = os.path.abspath(os.path.join(CURRENT_DIR, ".."))
OVERLAYS_DIR = os.path.join(ROOT_PROJECT_DIR, "deployments/sequencer/configs/overlays")
SEQUENCER_CONFIG_KEY = "sequencerConfig"


def _yq(args: list[str]) -> str:
    """Run a yq command and return stdout. Exits with a helpful message if yq is missing."""
    try:
        return subprocess.run(["yq"] + args, capture_output=True, text=True, check=True).stdout
    except FileNotFoundError:
        print("Error: 'yq' is not installed. Install it with:")
        print(
            "  wget -qO ~/.local/bin/yq https://github.com/mikefarah/yq/releases/latest/download/yq_linux_amd64 && chmod +x ~/.local/bin/yq"
        )
        print("  # or: brew install yq")
        sys.exit(1)


def is_sorted(filepath: str) -> bool:
    """Returns True if sequencerConfig keys are alphabetically sorted (or absent)."""
    result = _yq(
        [
            f"select(.config.{SEQUENCER_CONFIG_KEY} != null) | (.config.{SEQUENCER_CONFIG_KEY} | keys) == (.config.{SEQUENCER_CONFIG_KEY} | to_entries | map(.key))",
            filepath,
        ]
    ).strip()
    return result != "false"


def fix_file(filepath: str) -> None:
    """Sort sequencerConfig keys alphabetically in-place using yq."""
    _yq(["-i", f".config.{SEQUENCER_CONFIG_KEY} |= sort_keys(.)", filepath])


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
            if not is_sorted(filepath):
                fix_file(filepath)
                print(f"  Fixed: {os.path.relpath(filepath, ROOT_PROJECT_DIR)}")
    else:
        unsorted = [f for f in files if not is_sorted(f)]

        if unsorted:
            print(
                f"The following overlay files have {SEQUENCER_CONFIG_KEY} keys that are not"
                " alphabetically sorted:\n"
            )
            for filepath in unsorted:
                print(f"  {os.path.relpath(filepath, ROOT_PROJECT_DIR)}")
            sys.exit(1)

        print(f"All overlay {SEQUENCER_CONFIG_KEY} keys are alphabetically sorted.")


if __name__ == "__main__":
    main()
