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
        "--topology",
        default="single",
        type=str,
        choices=["single", "distributed"],
        help="Optional: Specify the system topology. (single"
    )
    # parser.add_argument(
    #     "--config-file",
    #     type=str,
    #     help="Optional: Path to sequencer configuration file."
    # )
    # parser.add_argument(
    #     "--env",
    #     default="dev",
    #     type=str,
    #     choices=["dev", "prod"],
    #     help="Optional: Specify the environment."
    # )

    return parser.parse_args()
