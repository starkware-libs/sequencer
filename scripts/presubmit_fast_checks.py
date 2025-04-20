#!/bin/env python3

"""
This script is meant to run a subset of the presubmit checks, the ones whose run time is "fast".
It can be used to run these checks locally or part of the CI.
"""

from abc import ABC, abstractmethod
from enum import Enum
from os import path
from named_todos import enforce_named_todos
from run_tests import run_test, BaseCommand
from typing import TypeVar
from utils import run_command

import argparse
import subprocess

SCRIPTS_LOCATION = path.dirname(__file__)

TCheck = TypeVar("TCheck", bound="Check")


class Check(ABC):
    def __init__(self):
        pass

    @classmethod
    def from_args(cls: type[TCheck], args: argparse.Namespace) -> TCheck:
        return cls()

    @classmethod
    def add_required_args(cls: type[TCheck], args_parser: argparse.ArgumentParser):
        pass

    @abstractmethod
    def run_check(self):
        pass


class RunTestsCheck(Check):
    def __init__(self, command: BaseCommand, from_commit_hash: str):
        self.command = command
        self.from_commit_hash = from_commit_hash

    @classmethod
    def add_required_args(cls, args_parser: argparse.ArgumentParser):
        add_from_commit_hash_arg(parser=args_parser)

    def run_check(self):
        print(f"Calling run_test with command: {self.command}")
        run_test(
            changes_only=True,
            commit_id=self.from_commit_hash,
            base_command=self.command,
            include_dependencies=False,
        )


class ExternalCommandCheck(Check):
    def __init__(self, commands: list[list[str]]):
        self.commands = commands

    def run_check(self):
        for cmd in self.commands:
            run_command(
                command=" ".join(cmd), allow_error=False, print_output_on_error=True
            )


class ClippyCheck(RunTestsCheck):
    def __init__(self, from_commit_hash: str):
        assert from_commit_hash, "from_commit_hash is required for clippy check."
        super().__init__(command=BaseCommand.CLIPPY, from_commit_hash=from_commit_hash)

    @classmethod
    def from_args(cls, args: argparse.Namespace):
        return ClippyCheck(from_commit_hash=args.from_commit_hash)


class DocCheck(RunTestsCheck):
    def __init__(self, from_commit_hash: str):
        assert from_commit_hash, "from_commit_hash is required for doc check."
        super().__init__(command=BaseCommand.DOC, from_commit_hash=from_commit_hash)

    @classmethod
    def from_args(cls, args: argparse.Namespace):
        return DocCheck(from_commit_hash=args.from_commit_hash)


class GitSubmodulesCheck(ExternalCommandCheck):
    def __init__(self):
        super().__init__(commands=[["git", "submodule", "status"]])


class CommitLintCheck(ExternalCommandCheck):
    def __init__(self, from_commit_hash: str, to_commit_hash: str):
        assert from_commit_hash, "from_commit_hash is required for commit lint check."
        assert to_commit_hash, "to_commit_hash is required for commit lint check."
        super().__init__(
            commands=[
                ["commitlint"] + ["--from", from_commit_hash] + ["--to", to_commit_hash]
            ]
        )

    @classmethod
    def add_required_args(cls, args_parser: argparse.ArgumentParser):
        add_from_commit_hash_arg(parser=args_parser)
        add_to_commit_hash_arg(parser=args_parser)

    @classmethod
    def from_args(cls, args: argparse.Namespace):
        return CommitLintCheck(
            from_commit_hash=args.from_commit_hash, to_commit_hash=args.to_commit_hash
        )


class TodosCheck(Check):
    def __init__(self, from_commit_hash: str):
        assert from_commit_hash, "from_commit_hash is required for TODOs check."
        self.from_commit_hash = from_commit_hash

    def run_check(self):
        enforce_named_todos(commit_id=self.from_commit_hash)

    @classmethod
    def add_required_args(cls, args_parser: argparse.ArgumentParser):
        add_from_commit_hash_arg(parser=args_parser)

    @classmethod
    def from_args(cls, args: argparse.Namespace):
        return TodosCheck(from_commit_hash=args.from_commit_hash)


class CargoLockCheck(ExternalCommandCheck):
    def __init__(self):
        super().__init__(
            commands=[
                ["cargo", "update", "-w", "--locked"],
                ["git", "diff", "--exit-code", "Cargo.lock"],
            ]
        )


# TODO(guy.f): There already exists a rust_fmt.sh script which is used locally. See if we can change
# the code in main.yml to use it. If so, replace this with the rust_fmt.sh script.
class RustFormatCheck(ExternalCommandCheck):
    def __init__(self, extra_rust_toolchains: str):
        assert (
            extra_rust_toolchains
        ), "extra_rust_toolchains is required for rust formatting checks."
        super().__init__(
            commands=[
                [
                    "cargo",
                    f"+{extra_rust_toolchains}",
                    "fmt",
                    "--all",
                    "--",
                    "--check",
                ]
            ]
        )

    @classmethod
    def add_required_args(cls, args_parser: argparse.ArgumentParser):
        add_rust_extra_toolchain_arg(parser=args_parser)

    @classmethod
    def from_args(cls, args: argparse.Namespace):
        return RustFormatCheck(extra_rust_toolchains=args.extra_rust_toolchains)


class TaploCheck(ExternalCommandCheck):
    def __init__(self):
        super().__init__(commands=[["bash", path.join(SCRIPTS_LOCATION, "taplo.sh")]])


class MacheteCheck(ExternalCommandCheck):
    def __init__(self):
        super().__init__(commands=[["cargo", "machete"]])


def add_argument_if_missing(parser: argparse.ArgumentParser, *args, **kwargs):
    existing_flags = {
        opt for action in parser._actions for opt in action.option_strings
    }

    # If any of the args already exist, do NOT add. Adding the same arg multiple times will result
    # in an exception when parsing the args.
    if not any(arg in existing_flags for arg in args):
        parser.add_argument(*args, **kwargs)


def add_from_commit_hash_arg(parser: argparse.ArgumentParser):
    add_argument_if_missing(
        parser,
        "--from_commit_hash",
        required=True,
        type=str,
        help="The commit hash of base, i.e. the code prior to the changes.",
    )


def add_to_commit_hash_arg(parser: argparse.ArgumentParser):
    add_argument_if_missing(
        parser,
        "--to_commit_hash",
        required=True,
        type=str,
        help="The commit hash of the top change, i.e. the most recent commit to be checked.",
    )


def add_rust_extra_toolchain_arg(parser: argparse.ArgumentParser):
    add_argument_if_missing(
        parser,
        "--extra_rust_toolchains",
        required=True,
        type=str,
        help="Extra rust toolchains to use. Required for the rust formatting checks.",
    )


def parse_args(all_checks: dict[str, type[Check]]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Presubmit script - fast parts.")
    subparsers = parser.add_subparsers(dest="command", required=True)

    all_sub_command_parser = subparsers.add_parser("all")

    for check_str, check in all_checks.items():
        check_sub_command_parser = subparsers.add_parser(check_str)
        check.add_required_args(check_sub_command_parser)
        check.add_required_args(all_sub_command_parser)

    return parser.parse_args()


def get_checks_to_run(
    args: argparse.Namespace, all_checks: dict[str, type[Check]]
) -> list[Check]:
    if args.command == "all":
        stages_to_run = all_checks.values()
    else:
        stages_to_run = [all_checks[args.command]]

    checks = []
    for stage in stages_to_run:
        checks.append(stage.from_args(args))

    return checks


def main():
    all_check_classes = [
        CommitLintCheck,
        GitSubmodulesCheck,
        TodosCheck,
        CargoLockCheck,
        RustFormatCheck,
        TaploCheck,
        MacheteCheck,
        ClippyCheck,
        DocCheck,
    ]

    all_checks_by_name = {check.__name__: check for check in all_check_classes}

    args = parse_args(all_checks=all_checks_by_name)

    checks = get_checks_to_run(args=args, all_checks=all_checks_by_name)

    for check in checks:
        check.run_check()


if __name__ == "__main__":
    main()
