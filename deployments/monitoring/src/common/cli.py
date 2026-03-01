import argparse


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
        "--dashboard-overrides-config-file",
        type=str,
        required=False,
        help="Path to YAML file with dashboard overrides (e.g., dashboard_name.field: value). Optional.",
    )
    parser.add_argument(
        "--alert-rules-overrides-config-file",
        type=str,
        required=True,
        help="Path to YAML file with alert rule overrides (e.g., alert_name.field: value). Required.",
    )

    args = parser.parse_args()

    assert (
        args.dev_dashboards_file or args.dev_alerts_file
    ), "At least one of --dev-dashboards-file or --dev-alerts-file must be provided."

    return args
