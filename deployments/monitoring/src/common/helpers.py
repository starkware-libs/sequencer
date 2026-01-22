import argparse
from enum import Enum

import colorlog


class EnvironmentName(Enum):
    DEV = "dev"
    INTEGRATION = "integration"
    TESTNET = "testnet"
    MAINNET = "mainnet"


# Translates the environment name to a suffix for alert filenames. We use the `mainnet` setting for development and the mainnet environment.
# The `testnet` setting is used for integration and testnet environments.
def alert_env_filename_suffix(env: EnvironmentName) -> str:
    env_to_alert_filename_suffix_mapping = {
        EnvironmentName.DEV: "mainnet",
        EnvironmentName.INTEGRATION: "testnet",
        EnvironmentName.TESTNET: "testnet",
        EnvironmentName.MAINNET: "mainnet",
    }
    return env_to_alert_filename_suffix_mapping[env]


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


def arg_parser() -> argparse.ArgumentParser:
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
        required=True,
        help="Provide Kubernetes namespace to inject into alert expressions.",
    )
    parser.add_argument(
        "--cluster",
        type=str,
        required=True,
        help="Provide Kubernetes cluster to inject into alert expressions.",
    )
    parser.add_argument(
        "--env",
        type=str,
        choices=[e.value for e in EnvironmentName],
        required=True,
    )
    parser.add_argument(
        "--dashboard-overrids-config-file",
        type=str,
        required=False,
        help="Path to YAML file with dashboard overrids (e.g., dashboard_name.field: value). Optional.",
    )
    parser.add_argument(
        "--alert-rules-overrids-config-file",
        type=str,
        required=True,
        help="Path to YAML file with alert rule overrids (e.g., alert_name.field: value). Required.",
    )

    args = parser.parse_args()

    assert (
        args.dev_dashboards_file or args.dev_alerts_file
    ), "At least one of --dev-dashboards-file or --dev-alerts-file must be provided."

    return args
