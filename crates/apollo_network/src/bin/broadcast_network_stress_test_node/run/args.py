import argparse


def add_shared_args_to_parser(parser: argparse.ArgumentParser):
    """
    Adds the arguments that are shared between the local and cluster deployment scripts
    """
    parser.add_argument(
        "--num-nodes", help="Number of nodes to run", type=int, default=3
    )
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
        default=100000,
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
        default=1000,
    )
    parser.add_argument(
        "--mode",
        help="The mode to use for the stress test.",
        choices=["all", "one", "rr", "explore"],
        default="all",
    )
    parser.add_argument(
        "--network-protocol",
        help="The network protocol to use for communication.",
        choices=["gossipsub", "sqmr", "reversed-sqmr", "propeller"],
        default="gossipsub",
    )
    parser.add_argument(
        "--broadcaster",
        help="In mode `one`, which node ID should do the broadcasting, default is the last node.",
        type=int,
        default=None,
    )
    parser.add_argument(
        "--round-duration-seconds",
        help="Duration each node broadcasts before switching (in seconds) - for RoundRobin mode",
        type=int,
        default=3,
    )
    parser.add_argument(
        "--quic",
        help="Sets the multi-addresses to use UDP/QUIC instead of TCP",
        action="store_true",
        default=False,
    )
    parser.add_argument(
        "--explore-cool-down-duration-seconds",
        help="Cool down duration between configuration changes in seconds - for Explore mode",
        type=int,
        default=100,
    )
    parser.add_argument(
        "--explore-run-duration-seconds",
        help="Duration to run each configuration in seconds - for Explore mode",
        type=int,
        default=100,
    )
    parser.add_argument(
        "--explore-min-throughput-byte-per-seconds",
        help="Minimum throughput in bytes per second - for Explore mode",
        type=float,
        default=100 * (1 << 10),  # 100 KB/s
    )
    parser.add_argument(
        "--explore-min-message-size-bytes",
        help="Minimum message size in bytes - for Explore mode",
        type=int,
        default=1 << 10,  # 1 KB
    )
    parser.add_argument(
        "--timeout",
        help="The timeout in seconds for the node. Note than when running in a cluster the pod will be redeployed.",
        type=int,
        default=7200,
    )


def get_arguments(
    id: int | None,
    metric_port: int,
    p2p_port: int,
    bootstrap_nodes: list[str],
    args: argparse.Namespace,
) -> list[tuple[str, str]]:
    result = [
        ("--metric-port", str(metric_port)),
        ("--p2p-port", str(p2p_port)),
        ("--bootstrap", ",".join(bootstrap_nodes)),
        ("--timeout", str(args.timeout)),
        ("--verbosity", str(args.verbosity)),
        ("--buffer-size", str(args.buffer_size)),
        ("--message-size-bytes", str(args.message_size_bytes)),
        ("--heartbeat-millis", str(args.heartbeat_millis)),
        ("--mode", str(args.mode)),
        ("--network-protocol", str(args.network_protocol)),
        (
            "--broadcaster",
            (
                str(args.broadcaster)
                if args.broadcaster is not None
                else str(args.num_nodes - 1)
            ),
        ),
        ("--round-duration-seconds", str(args.round_duration_seconds)),
        (
            "--explore-cool-down-duration-seconds",
            str(args.explore_cool_down_duration_seconds),
        ),
        ("--explore-run-duration-seconds", str(args.explore_run_duration_seconds)),
        (
            "--explore-min-throughput-byte-per-seconds",
            str(args.explore_min_throughput_byte_per_seconds),
        ),
        (
            "--explore-min-message-size-bytes",
            str(args.explore_min_message_size_bytes),
        ),
        ("--num-nodes", str(args.num_nodes)),
    ]
    if id is not None:
        result.insert(0, ("--id", str(id)))
    return result


def get_env_vars(
    id: int | None,
    metric_port: int,
    p2p_port: int,
    bootstrap_nodes: list[str],
    args: argparse.Namespace,
) -> list[dict[str, str]]:
    arguments = get_arguments(
        id=id,
        metric_port=metric_port,
        p2p_port=p2p_port,
        bootstrap_nodes=bootstrap_nodes,
        args=args,
    )

    env_vars = []

    # Convert arguments to environment variables
    for name, value in arguments:
        env_name = name[2:].replace("-", "_").upper()
        env_vars.append({"name": env_name, "value": str(value)})

    # Add latency and throughput if provided
    for arg_name, env_name in [("latency", "LATENCY"), ("throughput", "THROUGHPUT")]:
        value = getattr(args, arg_name)
        if value is not None:
            env_vars.append({"name": env_name, "value": str(value)})

    return env_vars
