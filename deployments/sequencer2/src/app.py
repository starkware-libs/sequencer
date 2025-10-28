import os
from pathlib import Path
from typing import Optional

from cdk8s import App, Chart, YamlOutputType
from constructs import Construct

from src.charts.grafana import GrafanaAlertRuleGroupApp, GrafanaDashboardApp
from src.charts.service import ServiceApp
from src.config.merger import merge_configs
from src.config.deployment import (
    GrafanaAlertRuleGroupConfig,
    GrafanaDashboardConfig,
)
from src.config.schema import DeploymentConfig as DeploymentSchema
from .helpers import generate_random_hash, sanitize_name
from .parser import argument_parser


class SequencerNode(Chart):
    def __init__(
        self,
        scope: Construct,
        name: str,
        namespace: str,
        monitoring: bool,
        service_config,
        common_config,
    ):
        super().__init__(scope, name, disable_resource_name_hashes=True, namespace=namespace)
        self.service = ServiceApp(
            self,
            name,
            namespace=namespace,
            deployment_config={
                "common": common_config,
                "service": service_config,
            },
            monitoring=monitoring,
        )


class SequencerMonitoring(Chart):
    def __init__(
        self,
        scope: Construct,
        id: str,
        cluster: str,
        namespace: str,
        grafana_dashboard: Optional[GrafanaDashboardConfig],
        grafana_alert_rule_group: Optional[GrafanaAlertRuleGroupConfig],
    ):
        super().__init__(scope, id, disable_resource_name_hashes=True, namespace=namespace)
        self.hash = generate_random_hash(from_string=f"{cluster}-{namespace}")

        if grafana_dashboard:
            self.dashboard = GrafanaDashboardApp(
                self,
                sanitize_name(f"dashboard-{self.hash}"),
                cluster=cluster,
                namespace=namespace,
                grafana_dashboard=grafana_dashboard,
            )

        if grafana_alert_rule_group:
            self.alert_rule_group = GrafanaAlertRuleGroupApp(
                self,
                sanitize_name(f"alert-rule-group-{self.hash}"),
                cluster=cluster,
                namespace=namespace,
                grafana_alert_rule_group=grafana_alert_rule_group,
            )


def main():
    args = argument_parser()

    # Validate monitoring arguments
    if args.monitoring_dashboard_file and not args.cluster:
        raise ValueError("--cluster is required when --monitoring-dashboard-file is provided.")

    app = App(yaml_output_type=YamlOutputType.FOLDER_PER_CHART_FILE_PER_RESOURCE)

    # --- Resolve base directory relative to main.py
    base_dir = Path(__file__).resolve().parent

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
        overlay_services_config_dir_path=str(overlay_services_config_dir) if overlay_services_config_dir else None,
    )

    # --- Prepare monitoring configs
    grafana_dashboard_config = (
        GrafanaDashboardConfig(args.monitoring_dashboard_file)
        if args.monitoring_dashboard_file
        else None
    )

    grafana_alert_rule_group_config = (
        GrafanaAlertRuleGroupConfig(args.monitoring_alerts_folder)
        if args.monitoring_alerts_folder
        else None
    )

    create_monitoring = bool(grafana_dashboard_config or grafana_alert_rule_group_config)

    # --- Create SequencerNode charts (one per service)
    namespace = sanitize_name(args.namespace)
    for service_cfg in deployment_config.services:
        SequencerNode(
            scope=app,
            name=sanitize_name(f"sequencer-{service_cfg.name}"),
            namespace=namespace,
            monitoring=create_monitoring,
            service_config=service_cfg,
            common_config=deployment_config.common,
        )

    # --- Create Grafana Monitoring chart
    if create_monitoring:
        SequencerMonitoring(
            scope=app,
            id="sequencer-monitoring",
            cluster=args.cluster,
            namespace=namespace,
            grafana_dashboard=grafana_dashboard_config,
            grafana_alert_rule_group=grafana_alert_rule_group_config,
        )

    # --- Synthesize manifests
    app.synth()
