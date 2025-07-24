import argparse


def add_broadcast_stress_test_node_arguments_to_parser(parser: argparse.ArgumentParser):
    parser.add_argument(
        "--verbosity",
        help="Verbosity level for logging (0: None, 1: ERROR, 2: WARN, 3: INFO, 4: DEBUG, 5..: TRACE)",
        type=int,
        default=2,
    )
    parser.add_argument(
        "--buffer-size",
        help="Buffer size to use by default.",
        type=int,
        default=10000,
    )
    parser.add_argument(
        "--message-size-bytes",
        help="Buffer size to use by default.",
        type=int,
        default=1 << 10,
    )
    parser.add_argument(
        "--heartbeat-millis",
        help="Number of milliseconds to wait between consecutive broadcasts.",
        type=int,
        default=1,
    )
    parser.add_argument(
        "--timeout",
        help="Number of seconds to run the node for.",
        type=int,
        default=3600,
    )


def get_arguments(
    id: int | None,
    metric_port: int,
    p2p_port: int,
    bootstrap: str,
    args: argparse.Namespace,
) -> list[tuple[str, str]]:
    result = [
        ("--metric-port", str(metric_port)),
        ("--p2p-port", str(p2p_port)),
        ("--bootstrap", str(bootstrap)),
        ("--verbosity", str(args.verbosity)),
        ("--buffer-size", str(args.buffer_size)),
        ("--message-size-bytes", str(args.message_size_bytes)),
        ("--heartbeat-millis", str(args.heartbeat_millis)),
        ("--timeout", str(args.timeout)),
    ]
    if id is not None:
        result.insert(0, ("--id", str(id)))
    return result
