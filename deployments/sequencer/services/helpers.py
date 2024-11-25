import argparse


def argument_parser():
    parser = argparse.ArgumentParser()

    parser.add_argument(
        "--config",
        required=False,
        type=str,
        default=None,
        help="Optional: Path to sequencer configuration file."
    )

    parser.add_argument(
        "--env",
        required=False,
        default="dev",
        type=str,
        help="Optional: Specify the enironment (e.g., dev, prod)"
    )

    parser.add_argument(
        "--namespace",
        required=False,
        type=str,
        default="default",
        help="Optional: Specify the Kubernetes namespace."
    )

    return parser.parse_args()

args = argument_parser()