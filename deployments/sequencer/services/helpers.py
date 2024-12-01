import argparse


def argument_parser():
    parser = argparse.ArgumentParser()

    parser.add_argument(
        "--namespace", required=True, type=str, help="Required: Specify the Kubernetes namespace."
    )
    parser.add_argument(
        "--config-file", type=str, help="Optional: Path to sequencer configuration file."
    )
    parser.add_argument(
        "--env", default="dev", type=str, help="Optional: Specify the enironment (e.g., dev, prod)"
    )

    return parser.parse_args()
