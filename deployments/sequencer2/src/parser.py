import argparse


def argument_parser():
    parser = argparse.ArgumentParser()
    parser.add_argument("--cluster", required=False, type=str, help="Provide the cluster name.")
    parser.add_argument("--namespace", required=True, type=str, help="Kubernetes namespace.")
    parser.add_argument(
        "--deployment-config-file", required=False, type=str, help="Path to deployment config file."
    )
    parser.add_argument(
        "--monitoring-dashboard-file",
        required=False,
        type=str,
        help="Path to Grafana dashboard JSON file.",
    )

    image_group = parser.add_mutually_exclusive_group()
    image_group.add_argument(
        "--deployment-image-tag",
        required=False,
        type=str,
        help="Apollo node image tag to fetch from GHCR (default: 'dev')",
    )
    image_group.add_argument(
        "--deployment-image",
        required=False,
        type=str,
        help="Full Docker image to use instead of default GHCR tag",
    )

    parser.add_argument(
        "--monitoring-alerts-folder",
        required=False,
        type=str,
        help="Path to Grafana alerts folder.",
    )
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

    return parser.parse_args()
