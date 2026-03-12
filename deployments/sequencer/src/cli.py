import argparse

# Attribute on the namespace used to track which dests have been set by UniqueStoreAction.
_UNIQUE_STORE_SEEN = "_UniqueStoreAction_seen"


class UniqueStoreAction(argparse.Action):
    """Allows an option to be specified at most once. Tracks whether the option was already
    specified on the command line, so uniqueness is enforced even when the first value
    equals the argument default (e.g. --layout hybrid --layout distributed)."""

    def __call__(self, parser, namespace, values, option_string=None):
        seen = getattr(namespace, _UNIQUE_STORE_SEEN, None)
        if seen is None:
            seen = set()
            setattr(namespace, _UNIQUE_STORE_SEEN, seen)
        if self.dest in seen:
            raise argparse.ArgumentError(self, "argument can only be specified once")
        seen.add(self.dest)
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
        default="hybrid",
        help="Layout name to use. Default: hybrid",
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
