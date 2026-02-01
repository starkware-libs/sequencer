#!/usr/bin/env python3

import json
import sys
from os import environ, getcwd
from os.path import commonpath, exists
from subprocess import run

# TODO(Nadin): Take the Rust toolchain version from CI configuration instead of hardcoding it here
RUST_TOOLCHAIN = "nightly-2024-04-29"


def project_root() -> str:
    return environ.get("CURSOR_PROJECT_DIR") or environ.get("CLAUDE_PROJECT_DIR") or getcwd()


def is_under_root(path: str, root: str) -> bool:
    try:
        return commonpath([path, root]) == root
    except ValueError:
        return False


def safe_run(command: list[str], cwd: str) -> None:
    try:
        run(command, cwd=cwd, check=True)
    except FileNotFoundError:
        print(f"Hook formatter missing: {command[0]}", file=sys.stderr)
    except Exception as exc:
        print(f"Hook formatter failed: {command} ({exc})", file=sys.stderr)


def format_rust(file_path: str, cwd: str) -> None:
    safe_run(
        [
            "cargo",
            f"+{RUST_TOOLCHAIN}",
            "fmt",
            "--",
            file_path,
        ],
        cwd=cwd,
    )


def format_python(cwd: str) -> None:
    safe_run(
        ["scripts/py_code_style.py", "--fix"],
        cwd=cwd,
    )


def main() -> int:
    try:
        payload = json.load(sys.stdin)
    except json.JSONDecodeError:
        return 0

    file_path = payload.get("file_path")
    if not file_path or not isinstance(file_path, str):
        return 0

    if not exists(file_path):
        return 0

    root = project_root()
    if not is_under_root(file_path, root):
        return 0

    if file_path.endswith(".rs"):
        format_rust(file_path, root)
    elif file_path.endswith(".py"):
        format_python(root)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
