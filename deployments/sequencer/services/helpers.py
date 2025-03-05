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
        required=False,
        type=str,
        action='append',
        help="Path to sequencer configuration file."
    )
    parser.add_argument(
        "--preset-file",
        required=False,
        type=str,
        help="Path to topology file."
    )

    return parser.parse_args()
