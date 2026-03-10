import argparse


class UniqueStoreAction(argparse.Action):
    """Allows an option to be specified at most once. Uses default-based check so arguments
    with a default (e.g. --layout default=consolidated) are not treated as already set."""

    def __call__(self, parser, namespace, values, option_string=None):
        current = getattr(namespace, self.dest, self.default)
        if current != self.default:
            raise argparse.ArgumentError(self, f"argument can only be specified once")
        setattr(namespace, self.dest, values)


def argument_parser():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--cluster",
        required=False,
        type=str,
        action=UniqueStoreAction,
        help="Provide the cluster name.",
    )
    parser.add_argument(
        "--namespace",
        required=True,
        type=str,
        action=UniqueStoreAction,
        help="Kubernetes namespace.",
    )
    parser.add_argument(
        "-l",
        "--layout",
        type=str,
        action=UniqueStoreAction,
        choices=["consolidated", "hybrid", "distributed"],
        default="consolidated",
        help="Layout name to use. Default: consolidated",
    )
    parser.add_argument(
        "-o",
        "--overlay",
        type=str,
        action="append",
        default=[],
        help="Overlay path(s) to apply, in order. Can be specified multiple times. "
        "Merged left-to-right (last wins). Must start with layout name.",
    )
    parser.add_argument(
        "--monitoring-dashboard-file",
        required=False,
        type=str,
        action=UniqueStoreAction,
        help="Path to Grafana dashboard JSON file.",
    )
    parser.add_argument(
        "--monitoring-alerts-folder",
        required=False,
        type=str,
        action=UniqueStoreAction,
        help="Path to Grafana alerts folder.",
    )
    parser.add_argument(
        "--image",
        required=False,
        type=str,
        action=UniqueStoreAction,
        help="Override image for all services. Format: 'repository:tag' or 'repository' (defaults to 'latest' tag).",
    )
    parser.add_argument(
        "-v",
        "--verbose",
        action="store_true",
        help="Enable verbose output including full tracebacks for errors.",
    )

    return parser.parse_args()
