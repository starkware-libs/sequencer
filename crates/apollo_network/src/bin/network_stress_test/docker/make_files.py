import os
import sys


def run_cmd(cmd: str, hint: str = "none", may_fail: bool = False):
    print(f"CMD: {cmd}", flush=True)
    result = os.system(cmd)
    if result != 0 and not may_fail:
        raise RuntimeError(
            f"Command failed with exit code {result}: {cmd}\nHint: {hint}"
        )


def main():

    # Get the directory of the current script
    current_dir = os.path.dirname(os.path.abspath(__file__))

    # Define the path to the .gitignore file
    gitignore_path = os.path.join(current_dir, ".gitignore")

    # Write the content to the .gitignore file
    with open(gitignore_path, "w") as f:
        f.write("docker-compose.yml\n")
        f.write("Dockerfile\n")

    print(f".gitignore file created at {gitignore_path}")


if __name__ == "__main__":
    main()
