import argparse


def argument_parser():
    parser = argparse.ArgumentParser()

    parser.add_argument("--namespace", required=True, type=str, help="Kubernetes namespace.")
    parser.add_argument(
        "--deployment-config-file", required=True, type=str, help="Path to deployment config file."
    )

    return parser.parse_args()
