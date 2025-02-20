import argparse


def argument_parser():
    parser = argparse.ArgumentParser()

    parser.add_argument(
        "--namespace",
        required=True,
        type=str,
        help="Required: Specify the Kubernetes namespace."
    )
    parser.add_argument(
        "--config-file",
        required=True,
        type=str,
        action='append',
        help="Optional: Path to sequencer configuration file. Can be used multiple times."
    )

    return parser.parse_args()
