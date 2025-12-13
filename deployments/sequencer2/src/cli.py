import argparse


def argument_parser():
    parser = argparse.ArgumentParser()
    parser.add_argument("--cluster", required=False, type=str, help="Provide the cluster name.")
    parser.add_argument("--namespace", required=True, type=str, help="Kubernetes namespace.")
    parser.add_argument(
        "-l",
        "--layout",
        type=str,
        choices=["consolidated", "hybrid", "hybrid2", "distributed"],
        default="consolidated",
        help="Layout name to use. Default: consolidated",
    )
    parser.add_argument(
        "-o",
        "--overlay",
        type=str,
        help="Optional overlay path to apply. Must start with layout name and use dot notation for nested paths (e.g., 'hybrid.sepolia-integration.node-01').",
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
    parser.add_argument(
        "--image",
        required=False,
        type=str,
        help="Override image for all services. Format: 'repository:tag' or 'repository' (defaults to 'latest' tag).",
    )

    return parser.parse_args()
