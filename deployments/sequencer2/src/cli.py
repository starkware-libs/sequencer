import argparse


def argument_parser():
    parser = argparse.ArgumentParser()
    parser.add_argument("--cluster", required=False, type=str, help="Provide the cluster name.")
    parser.add_argument("--namespace", required=True, type=str, help="Kubernetes namespace.")
    parser.add_argument(
        "-l",
        "--layout",
        type=str,
        choices=["consolidated", "hybrid", "distributed"],
        default="consolidated",
        help="Layout name to use. Default: consolidated",
    )
    parser.add_argument(
        "-o",
        "--overlay",
        type=str,
        choices=["dev", "integration", "prod"],
        help="Optional environment overlay to apply",
    )
    parser.add_argument(
        "--monitoring-dashboard-file",
        required=False,
        type=str,
        help="Path to Grafana dashboard JSON file.",
    )
    parser.add_argument(
        "--monitoring-alerts-folder",
        required=False,
        type=str,
        help="Path to Grafana alerts folder.",
    )

    return parser.parse_args()
