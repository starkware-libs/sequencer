import os
import subprocess
from typing import List

from git import Repo


def git_files(extension: str) -> List[str]:
    """
    Returns a list of files in the current git repository with the specified extension.
    """
    repo = Repo(".")
    return [
        item.path for item in repo.commit().tree.traverse() if item.path.endswith(f".{extension}")
    ]


def run_command(
    command: str, allow_error: bool = False, print_output_on_error: bool = False
) -> List[str]:
    """
    Runs a bash command and returns the output as a list of lines.
    """
    try:
        command_output = (
            subprocess.check_output(command, shell=True, cwd=os.getcwd())
            .decode("utf-8")
            .splitlines()
        )
        output_lines = "\n".join(command_output)
        print(f"Command '{command}' output:\n{output_lines}")
        return command_output
    except subprocess.CalledProcessError as error:
        if print_output_on_error:
            print(f"Command '{command}' output:\n{error.output.decode()}")

        if not allow_error:
            raise
        print(f"Command '{command}' hit error: {error=}.")
        return str(error).splitlines()
