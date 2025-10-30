import os
from pathlib import Path

from cdk8s import App, YamlOutputType

from src.charts.monitoring import MonitoringChart
from src.charts.node import SequencerNodeChart
from src.cli import argument_parser
from src.config.loaders import (
    GrafanaAlertRuleGroupConfigLoader,
    GrafanaDashboardConfigLoader,
)
from src.config.merger import merge_configs
from src.config.schema import DeploymentConfig as DeploymentSchema
from src.utils import sanitize_name


def main():
    args = argument_parser()

    # Validate monitoring arguments
    if args.monitoring_dashboard_file and not args.cluster:
        raise ValueError("--cluster is required when --monitoring-dashboard-file is provided.")

    app = App(yaml_output_type=YamlOutputType.FOLDER_PER_CHART_FILE_PER_RESOURCE)

    # --- Resolve base directory relative to main.py
    base_dir = Path(__file__).resolve().parents[1]

    # --- Layout (base) config paths
    layout_common_config = base_dir / "configs" / "layouts" / args.layout / "common.yaml"
    layout_services_config_dir = base_dir / "configs" / "layouts" / args.layout / "services"

    # --- Overlay config paths (optional)
    overlay_common_config = None
    overlay_services_config_dir = None
    if args.overlay:
        overlay_common_config = (
            base_dir / "configs" / "overlays" / args.layout / args.overlay / "common.yaml"
        )
        overlay_services_config_dir = (
            base_dir / "configs" / "overlays" / args.layout / args.overlay / "services"
        )

    # --- Merge layout + overlay configs (validated Pydantic model)
    deployment_config: DeploymentSchema = merge_configs(
        layout_common_config_path=str(layout_common_config),
        layout_services_config_dir_path=str(layout_services_config_dir),
        overlay_common_config_path=str(overlay_common_config) if overlay_common_config else None,
        overlay_services_config_dir_path=(
            str(overlay_services_config_dir) if overlay_services_config_dir else None
        ),
    )

    # --- Prepare monitoring configs
    grafana_dashboard_config = (
        GrafanaDashboardConfigLoader(args.monitoring_dashboard_file)
        if args.monitoring_dashboard_file
        else None
    )

    grafana_alert_rule_group_config = (
        GrafanaAlertRuleGroupConfigLoader(args.monitoring_alerts_folder)
        if args.monitoring_alerts_folder
        else None
    )

    create_monitoring = bool(grafana_dashboard_config or grafana_alert_rule_group_config)

    # --- Create SequencerNodeChart charts (one per service)
    namespace = sanitize_name(args.namespace)
    for service_cfg in deployment_config.services:
        SequencerNodeChart(
            scope=app,
            name=sanitize_name(f"sequencer-{service_cfg.name}"),
            namespace=namespace,
            monitoring=create_monitoring,
            common_config=deployment_config.common,
            service_config=service_cfg,
        )

    # --- Create Monitoring chart
    if create_monitoring:
        MonitoringChart(
            scope=app,
            id="sequencer-monitoring",
            cluster=args.cluster,
            namespace=namespace,
            grafana_dashboard=grafana_dashboard_config,
            grafana_alert_rule_group=grafana_alert_rule_group_config,
        )

    # --- Synthesize manifests
    app.synth()
