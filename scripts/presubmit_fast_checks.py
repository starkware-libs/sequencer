#!/bin/env python3

"""
This script is meant to run a subset of the presubmit checks, the ones whose run time is "fast".
It can be used to run these checks locally or part of the CI.
"""

import argparse
from abc import ABC, abstractmethod
from enum import Enum
from os import path
from typing import List, Type, TypeVar

from named_todos import enforce_named_todos
from run_tests import BaseCommand, run_test
from utils import run_command

SCRIPTS_LOCATION = path.dirname(__file__)

TCheck = TypeVar("TCheck", bound="Check")


class PresubmitArg(Enum):
    TO_COMMIT_HASH = "The commit hash of base, i.e. the code prior to the changes."

    FROM_COMMIT_HASH = (
        "The commit hash of the top change, i.e. the most recent commit to be checked."
    )

    EXTRA_RUST_TOOLCHAINS = "Extra rust toolchains to use. Required for the rust formatting checks."

    def add_args(self, parser: argparse.ArgumentParser) -> None:
        parser.add_argument(f"--{self.name.lower()}", required=True, type=str, help=self.value)


class Check(ABC):
    def __init__(self) -> None:
        pass

    @classmethod
    def from_args(cls: type[TCheck], args: argparse.Namespace) -> TCheck:
        return cls()

    @classmethod
    def required_args(cls: type[TCheck]) -> set[PresubmitArg]:
        return set()

    @abstractmethod
    def run_check(self) -> None:
        pass


class RunTestsCheck(Check):
    def __init__(self, command: BaseCommand, from_commit_hash: str):
        self.command = command
        self.from_commit_hash = from_commit_hash

    @classmethod
    def required_args(cls: type[TCheck]) -> set[PresubmitArg]:
        return {PresubmitArg.FROM_COMMIT_HASH}

    def run_check(self) -> None:
        print(f"Calling run_test with command: {self.command}")
        run_test(
            changes_only=True,
            commit_id=self.from_commit_hash,
            base_command=self.command,
            include_dependencies=False,
            is_nightly=False,
        )


class ExternalCommandCheck(Check):
    def __init__(self, commands: list[list[str]]):
        self.commands = commands

    def run_check(self) -> None:
        for cmd in self.commands:
            run_command(command=" ".join(cmd), allow_error=False, print_output_on_error=True)


class ClippyCheck(RunTestsCheck):
    def __init__(self, from_commit_hash: str):
        assert from_commit_hash, "from_commit_hash is required for clippy check."
        super().__init__(command=BaseCommand.CLIPPY, from_commit_hash=from_commit_hash)

    @classmethod
    def from_args(cls, args: argparse.Namespace) -> "ClippyCheck":
        return ClippyCheck(from_commit_hash=args.from_commit_hash)


class DocCheck(RunTestsCheck):
    def __init__(self, from_commit_hash: str):
        assert from_commit_hash, "from_commit_hash is required for doc check."
        super().__init__(command=BaseCommand.DOC, from_commit_hash=from_commit_hash)

    @classmethod
    def from_args(cls, args: argparse.Namespace) -> "DocCheck":
        return DocCheck(from_commit_hash=args.from_commit_hash)


class GitSubmodulesCheck(ExternalCommandCheck):
    def __init__(self) -> None:
        super().__init__(commands=[["git", "submodule", "status"]])


class CommitLintCheck(ExternalCommandCheck):
    def __init__(self, from_commit_hash: str, to_commit_hash: str):
        assert from_commit_hash, "from_commit_hash is required for commit lint check."
        assert to_commit_hash, "to_commit_hash is required for commit lint check."
        super().__init__(
            commands=[["commitlint"] + ["--from", from_commit_hash] + ["--to", to_commit_hash]]
        )

    @classmethod
    def required_args(cls: type[TCheck]) -> set[PresubmitArg]:
        return {PresubmitArg.FROM_COMMIT_HASH, PresubmitArg.TO_COMMIT_HASH}

    @classmethod
    def from_args(cls, args: argparse.Namespace) -> "CommitLintCheck":
        return CommitLintCheck(
            from_commit_hash=args.from_commit_hash, to_commit_hash=args.to_commit_hash
        )


class TodosCheck(Check):
    def __init__(self, from_commit_hash: str):
        assert from_commit_hash, "from_commit_hash is required for TODOs check."
        self.from_commit_hash = from_commit_hash

    def run_check(self) -> None:
        enforce_named_todos(commit_id=self.from_commit_hash)

    @classmethod
    def required_args(cls: type[TCheck]) -> set[PresubmitArg]:
        return {PresubmitArg.FROM_COMMIT_HASH}

    @classmethod
    def from_args(cls, args: argparse.Namespace) -> "TodosCheck":
        return TodosCheck(from_commit_hash=args.from_commit_hash)


class CargoLockCheck(ExternalCommandCheck):
    def __init__(self) -> None:
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
    def required_args(cls: type[TCheck]) -> set[PresubmitArg]:
        return {PresubmitArg.EXTRA_RUST_TOOLCHAINS}

    @classmethod
    def from_args(cls, args: argparse.Namespace) -> "RustFormatCheck":
        return RustFormatCheck(extra_rust_toolchains=args.extra_rust_toolchains)


class TaploCheck(ExternalCommandCheck):
    def __init__(self) -> None:
        super().__init__(commands=[["bash", path.join(SCRIPTS_LOCATION, "taplo.sh")]])


class MacheteCheck(ExternalCommandCheck):
    def __init__(self) -> None:
        super().__init__(commands=[["cargo", "machete"]])


def parse_args(all_checks: dict[str, type[Check]]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Presubmit script - fast parts.")
    subparsers = parser.add_subparsers(dest="command", required=True)

    all_args = set()
    for check_str, check in all_checks.items():
        check_required_args = check.required_args()
        all_args.update(check_required_args)

        check_sub_command_parser = subparsers.add_parser(check_str)
        for arg in check_required_args:
            arg.add_args(check_sub_command_parser)

    all_sub_command_parser = subparsers.add_parser("all")
    for arg in all_args:
        arg.add_args(all_sub_command_parser)

    return parser.parse_args()


def get_checks_to_run(args: argparse.Namespace, all_checks: dict[str, type[Check]]) -> list[Check]:
    if args.command == "all":
        stages_to_run = list(all_checks.values())
    else:
        stages_to_run = [all_checks[args.command]]

    checks = []
    for stage in stages_to_run:
        checks.append(stage.from_args(args))

    return checks


def main() -> None:
    all_check_classes: List[Type[Check]] = [
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
