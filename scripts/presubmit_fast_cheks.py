#!/bin/env python3

"""
This script is meant to run a subset of the presubmit checks, the ones whose run time is "fast".
It can be used to run these checks locally or part of the CI.

For local use it should be wrapped by a caller which computes the relevant commits.
"""

from abc import ABC, abstractmethod
from enum import Enum
from os import path

import argparse
import subprocess

SCRIPTS_LOCATION = path.dirname(__file__)


class RunModes(Enum):
    LOCAL_PRESUBMIT = "local_presubmit"
    CI_PRESUBMIT = "ci_presubmit"
    # TODO(guy.f): See if we want to extend this script to cover the actions only run for push.
    # CI_PUSH = "ci_push"


class Check(ABC):
    def __init__(self):
        pass

    @classmethod
    def from_args(cls, args):
        return cls()

    @abstractmethod
    def get_cmds(self):
        pass


class RunTestsCheck(Check):
    def __init__(self, run_test_args: list[str]):
        self.run_test_args = run_test_args

    def get_cmds(self):
        return [["python", SCRIPTS_LOCATION + "/run_tests.py"] + self.run_test_args]


class ClippyCheck(RunTestsCheck):
    def __init__(self, from_commit_hash: str):
        assert from_commit_hash, "from_commit_hash is required for clippy check."
        super().__init__(
            ["--command", "clippy", "--changes_only", "--commit_id", from_commit_hash]
        )

    @classmethod
    def from_args(cls, args):
        return ClippyCheck(args.from_commit_hash)


class DocCheck(RunTestsCheck):
    def __init__(self, from_commit_hash: str):
        assert from_commit_hash, "from_commit_hash is required for doc check."
        super().__init__(
            ["--command", "doc", "--changes_only", "--commit_id", from_commit_hash]
        )

    @classmethod
    def from_args(cls, args):
        return DocCheck(args.from_commit_hash)


class GitSubmodulesCheck(Check):
    def get_cmds(self):
        return [["git", "submodule", "status"]]


class CommitLintCheck(Check):
    def __init__(self, from_commit_hash: str, to_commit_hash: str):
        assert from_commit_hash, "from_commit_hash is required for commit lint check."
        assert to_commit_hash, "to_commit_hash is required for commit lint check."
        self.from_commit_hash = from_commit_hash
        self.to_commit_hash = to_commit_hash

    @classmethod
    def from_args(cls, args):
        return CommitLintCheck(args.from_commit_hash, args.to_commit_hash)

    def get_cmds(self):
        return [
            ["commitlint"]
            + ["--from", self.from_commit_hash]
            + ["--to", self.to_commit_hash]
        ]


class TodosCheck(Check):
    def __init__(self, from_commit_hash: str):
        assert from_commit_hash, "from_commit_hash is required for TODOs check."
        self.from_commit_hash = from_commit_hash

    @classmethod
    def from_args(cls, args):
        return TodosCheck(args.from_commit_hash)

    def get_cmds(self):
        return [
            ["python", SCRIPTS_LOCATION + "/named_todos.py"]
            + ["--commit_id", self.from_commit_hash]
        ]


class CargoLockCheck(Check):
    def get_cmds(self):
        return [
            ["cargo", "update", "-w", "--locked"],
            ["git", "diff", "--exit-code", "Cargo.lock"],
        ]


class RustFormatCheck(Check):
    def __init__(self, extra_rust_toolchains: str):
        assert (
            extra_rust_toolchains
        ), "extra_rust_toolchains is required for rust formatting checks."
        self.extra_rust_toolchains = extra_rust_toolchains

    @classmethod
    def from_args(cls, args):
        return RustFormatCheck(args.extra_rust_toolchains)

    def get_cmds(self):
        return [
            ["cargo", f"+{self.extra_rust_toolchains}", "fmt", "--all", "--", "--check"]
        ]


class TaploCheck(Check):
    def get_cmds(self):
        return [["bash", SCRIPTS_LOCATION + "/taplo.sh"]]


class MacheteCheck(Check):
    def get_cmds(self):
        return [["cargo", "machete"]]


class DummyCheck(Check):
    def get_cmds(self):
        return [["echo", "dummy test"]]


class FailingCheck(Check):
    def get_cmds(self):
        return ["fail"]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Presubmit script - fast parts.")

    parser.add_argument(
        "--mode",
        required=True,
        choices=[mode.value for mode in RunModes],
        help="Code inspection command to run.",
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

    parser.add_argument("--push_request_title", type=str, help="The title of the PR.")

    return parser.parse_args()


def GetChecksForMode(args: argparse.Namespace) -> list[Check]:
    mode = RunModes(args.mode)
    match mode:
        case RunModes.LOCAL_PRESUBMIT | RunModes.CI_PRESUBMIT:
            return [
                CommitLintCheck.from_args(args),
                GitSubmodulesCheck.from_args(args),
                TodosCheck.from_args(args),
                CargoLockCheck.from_args(args),
                RustFormatCheck.from_args(args),
                TaploCheck.from_args(args),
                MacheteCheck.from_args(args),
                ClippyCheck.from_args(args),
                DocCheck.from_args(args),
            ]
        case _:
            print(f"Invalid mode: {mode}. Not running any checks.")
            exit(1)


def main():
    args = parse_args()
    print(RunModes(args.mode))

    checks = GetChecksForMode(args)

    for check in checks:
        for cmd in check.get_cmds():
            print(f"Running: {' '.join(cmd)}", flush=True)
            subprocess.run(cmd, check=True)


if __name__ == "__main__":
    main()
