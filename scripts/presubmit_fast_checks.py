#!/bin/env python3

"""
This script is meant to run a subset of the presubmit checks, the ones whose run time is "fast".
It can be used to run these checks locally or part of the CI.
"""

from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from enum import Enum
from os import path
from run_tests import run_test, BaseCommand

import argparse
import subprocess
import utils

SCRIPTS_LOCATION = path.dirname(__file__)


class Check(ABC):
    def __init__(self):
        pass

    @classmethod
    def from_args(cls, args):
        return cls()

    @abstractmethod
    def run_check(self):
        pass

@dataclass
class RunTestsCheck(Check):
    command: BaseCommand
    from_commit_hash: str
    
    def run_check(self):
        assert self.from_commit_hash, "from_commit_hash is required for run_tests checks."
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
            utils.run_command(
                " ".join(cmd), allow_error=False, print_output_on_error=True
            )

@dataclass
class ClippyCheck(RunTestsCheck):
    command: BaseCommand = field(init=False, default=BaseCommand.CLIPPY)

    @classmethod
    def from_args(cls, args):
        return ClippyCheck(args.from_commit_hash)


class DocCheck(RunTestsCheck):
    command: BaseCommand = field(init=False, default=BaseCommand.DOC)

    @classmethod
    def from_args(cls, args):
        return DocCheck(args.from_commit_hash)


class GitSubmodulesCheck(ExternalCommandCheck):
    def __init__(self):
        super().__init__([["git", "submodule", "status"]])


class CommitLintCheck(ExternalCommandCheck):
    def __init__(self, from_commit_hash: str, to_commit_hash: str):
        assert from_commit_hash, "from_commit_hash is required for commit lint check."
        assert to_commit_hash, "to_commit_hash is required for commit lint check."
        super().__init__(
            [["commitlint"] + ["--from", from_commit_hash] + ["--to", to_commit_hash]]
        )

    @classmethod
    def from_args(cls, args):
        return CommitLintCheck(args.from_commit_hash, args.to_commit_hash)


class TodosCheck(ExternalCommandCheck):
    def __init__(self, from_commit_hash: str):
        assert from_commit_hash, "from_commit_hash is required for TODOs check."
        super().__init__(
            [
                ["python", SCRIPTS_LOCATION + "/named_todos.py"]
                + ["--commit_id", from_commit_hash]
            ]
        )

    @classmethod
    def from_args(cls, args):
        return TodosCheck(args.from_commit_hash)


class CargoLockCheck(ExternalCommandCheck):
    def __init__(self):
        super().__init__(
            [
                ["cargo", "update", "-w", "--locked"],
                ["git", "diff", "--exit-code", "Cargo.lock"],
            ]
        )


class RustFormatCheck(ExternalCommandCheck):
    def __init__(self, extra_rust_toolchains: str):
        assert (
            extra_rust_toolchains
        ), "extra_rust_toolchains is required for rust formatting checks."
        super().__init__(
            [
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
    def from_args(cls, args):
        return RustFormatCheck(args.extra_rust_toolchains)


class TaploCheck(ExternalCommandCheck):
    def __init__(self):
        super().__init__([["bash", SCRIPTS_LOCATION + "/taplo.sh"]])


class MacheteCheck(ExternalCommandCheck):
    def __init__(self):
        super().__init__([["cargo", "machete"]])


all_checks = [
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


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Presubmit script - fast parts.")

    parser.add_argument(
        "--stage",
        choices=[check.__name__ for check in all_checks],
        help="Which stage of the presubmit to run, if not set runs all stages.",
    )
    parser.add_argument(
        "--extra_rust_toolchains",
        type=str,
        help="Extra rust toolchains to use. Required for the rust formatting checks.",
    )
    parser.add_argument(
        "--from_commit_hash",
        type=str,
        help="The commit hash of base, i.e. the code prior to the changes.",
    )
    parser.add_argument(
        "--to_commit_hash",
        type=str,
        help="The commit hash of the top change, i.e. the most recent commit to be checked.",
    )

    return parser.parse_args()


def GetChecksToRun(args: argparse.Namespace) -> list[Check]:
    if args.stage is not None:
        stages_to_run = [globals()[args.stage]]
    else:
        stages_to_run = all_checks

    checks = []
    for stage in stages_to_run:
        checks.append(stage.from_args(args))

    return checks


def main():
    args = parse_args()

    checks = GetChecksToRun(args)

    for check in checks:
        check.run_check()


if __name__ == "__main__":
    main()
