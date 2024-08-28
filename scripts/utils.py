import os
import subprocess
from typing import List

def run_command(command: str, allow_error: bool = False) -> List[str]:
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
        if not allow_error:
            raise
        print(f"Command '{command}' hit error: {error=}.")
        return str(error).splitlines()
