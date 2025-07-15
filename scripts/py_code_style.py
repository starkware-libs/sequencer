#!/usr/bin/env python3.9

import argparse
import os
import subprocess

from utils import git_files

ROOT_PROJECT_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run Python code style checks.")
    parser.add_argument(
        "--fix", action="store_true", help="Fix mode. Runs only fixable checks, in fix mode."
    )
    return parser.parse_args()


def run_black(fix: bool):
    command = ["black", "-l", "100", "-t", "py37", ROOT_PROJECT_DIR]
    if not fix:
        command += ["--check", "--diff", "--color"]
    subprocess.check_output(command)


def run_isort(fix: bool):
    command = ["isort", "--settings-path", ROOT_PROJECT_DIR, ROOT_PROJECT_DIR]
    if not fix:
        command.append("-c")
    subprocess.check_output(command)


def run_autoflake(fix: bool):
    files = git_files("py")
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
    run_autoflake(fix=args.fix)
    run_black(fix=args.fix)
    run_isort(fix=args.fix)


if __name__ == "__main__":
    main()
