import subprocess
from tests_utils import get_workspace_tree


def verify_no_orphans():
    crates = get_workspace_tree()
    for crate in crates:
        print(f"Checking for orphans in {crate}...")
        command = f"cargo modules orphans --all-features --cfg-test --package {crate}"
        try:
            subprocess.check_output(args=command, shell=True).decode("utf-8")
        except subprocess.CalledProcessError:
            print(
                "ERROR, possibly due to both binary and library in crate. Analyzing only library..."
            )
            subprocess.check_output(args=command + " --lib", shell=True).decode("utf-8")


if __name__ == "__main__":
    verify_no_orphans()
