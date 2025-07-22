import argparse

import colorlog


def get_logger(name: str = __name__, debug: bool = False) -> colorlog.getLogger:
    message_color = "light_white"
    time_color = "light_black"
    name_color = "light_purple"

    logger = colorlog.getLogger(name=name)

    if logger.hasHandlers():
        logger.handlers.clear()

    handler = colorlog.StreamHandler()
    handler.setFormatter(
        colorlog.ColoredFormatter(
            "%(time_log_color)s%(asctime)s %(name_log_color)s%(name)s %(log_color)s%(levelname)s%(reset)s %(message_log_color)s%(message)s",
            reset=True,
            log_colors={
                "DEBUG": "blue",
                "INFO": "green",
                "WARNING": "yellow",
                "ERROR": "red",
                "CRITICAL": "bold_red",
            },
            secondary_log_colors={
                "message": {
                    "DEBUG": f"{message_color}",
                    "INFO": f"{message_color}",
                    "WARNING": f"{message_color}",
                    "ERROR": f"{message_color}",
                    "CRITICAL": f"{message_color}",
                },
                "time": {
                    "DEBUG": f"{time_color}",
                    "INFO": f"{time_color}",
                    "WARNING": f"{time_color}",
                    "ERROR": f"{time_color}",
                    "CRITICAL": f"{time_color}",
                },
                "name": {
                    "DEBUG": f"{name_color}",
                    "INFO": f"{name_color}",
                    "WARNING": f"{name_color}",
                    "ERROR": f"{name_color}",
                    "CRITICAL": f"{name_color}",
                },
            },
        )
    )
    logger.addHandler(handler)
    logger.setLevel(colorlog.DEBUG if debug else colorlog.INFO)

    return logger


def arg_parser() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Build And Upload Grafana Alerts")
    parser.add_argument(
        "--dev-dashboards-file",
        type=str,
        help="Path to the dev json file.",
    )
    parser.add_argument(
        "--dev-alerts-file",
        type=str,
        help="Path to the dev json file.",
    )
    parser.add_argument(
        "--debug",
        action="store_true",
        help="Enable debug logging. Default is False",
    )
    parser.add_argument(
        "--grafana-url",
        type=str,
        default="http://localhost:3000",
        help="Grafana URL. Default is http://localhost:3000",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Dry run, do not upload alerts to Grafana. Default is False",
    ),
    parser.add_argument(
        "--out-dir",
        type=str,
        default="./out",
        help="Output directory. Default is ./out",
    )
    parser.add_argument(
        "--folder-uid",
        type=str,
        default="",
        help='Provide Grafana folder_uid for the alerts. Default is ""',
    )
    parser.add_argument(
        "--datasource-uid",
        type=str,
        default="PBFA97CFB590B2093",
        help='Provide Prometheus datasource UID for the alerts. Default is "PBFA97CFB590B2093"',
    )
    parser.add_argument(
        "--namespace",
        type=str,
        help="Provide Kubernetes namespace to inject into alert expressions.",
    )
    parser.add_argument(
        "--cluster",
        type=str,
        help="Provide Kubernetes cluster to inject into alert expressions.",
    )
    parser.add_argument(
        "--env",
        type=str,
        choices=["dev", "integration", "testnet", "mainnet"],
        required=True,
    )

    args = parser.parse_args()

    assert not (
        (args.namespace and not args.cluster) or (args.cluster and not args.namespace)
    ), "If a namespace is provided, a cluster must also be provided, and vice versa."

    return args
