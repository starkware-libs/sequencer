#!/usr/bin/env python3
import argparse
import subprocess
import sys

import logging


# --- Helper Functions ---
def init_logging(verbose: bool):
    """Sets up the logging configuration."""
    logging.basicConfig(
        level=logging.DEBUG if verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
        datefmt="%Y-%m-%d %H:%M:%S",
    )


def run_kubectl(args):
    """Executes kubectl commands safely."""
    cmd = ["kubectl"] + args
    try:
        result = subprocess.run(
            cmd,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        return result.stdout
    except subprocess.CalledProcessError as e:
        print(f"Error running {' '.join(cmd)}:\n{e.stderr}", file=sys.stderr)
        return ""


# --- Main Entry Point ---
def main():
    parser = argparse.ArgumentParser(
        description="List pod container images across contexts and namespaces."
    )
    parser.add_argument(
        "--verbose",
        action="store_true",
        help="Enable verbose logging.",
    )

    args = parser.parse_args()
    init_logging(args.verbose)

    logging.info("Script initialized. No logic implemented yet.")


if __name__ == "__main__":
    main()
