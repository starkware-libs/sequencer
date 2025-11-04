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
    """Main entry point for CDK8s application."""
    args = argument_parser()
    _validate_args(args)

    app = App(yaml_output_type=YamlOutputType.FOLDER_PER_CHART_FILE_PER_RESOURCE)
    base_dir = _get_base_dir()

    # Load configuration
    deployment_config = _load_deployment_config(base_dir, args.layout, args.overlay)

    # Prepare monitoring
    monitoring_configs = _prepare_monitoring_configs(args)
    namespace = sanitize_name(args.namespace)

    # Create charts
    _create_service_charts(app, deployment_config, namespace, monitoring_configs["enabled"])
    _create_monitoring_chart(app, namespace, args.cluster, monitoring_configs)

    # Synthesize manifests
    app.synth()


def _validate_args(args):
    """Validate CLI arguments."""
    if args.monitoring_dashboard_file and not args.cluster:
        raise ValueError("--cluster is required when --monitoring-dashboard-file is provided.")


def _get_base_dir() -> Path:
    """Resolve base directory relative to main.py."""
    return Path(__file__).resolve().parents[1]


def _get_config_paths(
    base_dir: Path, layout: str, overlay: str | None
) -> tuple[Path, Path, Path | None, Path | None]:
    """Get layout and overlay config paths.

    Overlay path must start with the layout name and use dot notation for nested paths:
    - 'hybrid.sepolia-integration.node-01' -> configs/overlays/hybrid/sepolia-integration/node-01/
    - 'hybrid.sepolia-integration.node-02' -> configs/overlays/hybrid/sepolia-integration/node-02/
    """
    layout_common = base_dir / "configs" / "layouts" / layout / "common.yaml"
    layout_services = base_dir / "configs" / "layouts" / layout / "services"

    overlay_common = None
    overlay_services = None
    if overlay:
        # Split dot-separated path into segments
        # e.g., "hybrid.sepolia-integration.node-01" -> ["hybrid", "sepolia-integration", "node-01"]
        overlay_path_segments = overlay.split(".")

        # First segment must be the layout name
        if not overlay_path_segments or overlay_path_segments[0] != layout:
            raise ValueError(
                f"Overlay path '{overlay}' must start with the layout name '{layout}'. "
                f"Example: '{layout}.sepolia-integration.node-01'"
            )

        # Build overlay path: configs/overlays/{layout}/{segment2}/{segment3}/...
        # Skip first segment (layout) since it's already in the path
        overlay_base_path = base_dir / "configs" / "overlays" / layout
        for segment in overlay_path_segments[1:]:
            overlay_base_path = overlay_base_path / segment

        overlay_services = overlay_base_path / "services"

        # common.yaml is optional in overlay paths
        overlay_common_path = overlay_base_path / "common.yaml"
        overlay_common = overlay_common_path if overlay_common_path.exists() else None

    return (layout_common, layout_services, overlay_common, overlay_services)


def _load_deployment_config(base_dir: Path, layout: str, overlay: str | None) -> DeploymentSchema:
    """Load and merge deployment configuration."""
    layout_common, layout_services, overlay_common, overlay_services = _get_config_paths(
        base_dir, layout, overlay
    )

    return merge_configs(
        layout_common_config_path=str(layout_common),
        layout_services_config_dir_path=str(layout_services),
        overlay_common_config_path=str(overlay_common) if overlay_common else None,
        overlay_services_config_dir_path=str(overlay_services) if overlay_services else None,
    )


def _prepare_monitoring_configs(args) -> dict:
    """Prepare monitoring configurations."""
    dashboard_config = (
        GrafanaDashboardConfigLoader(args.monitoring_dashboard_file)
        if args.monitoring_dashboard_file
        else None
    )

    alerts_config = (
        GrafanaAlertRuleGroupConfigLoader(args.monitoring_alerts_folder)
        if args.monitoring_alerts_folder
        else None
    )

    return {
        "dashboard": dashboard_config,
        "alerts": alerts_config,
        "enabled": bool(dashboard_config or alerts_config),
    }


def _create_service_charts(
    app: App,
    deployment_config: DeploymentSchema,
    namespace: str,
    monitoring_enabled: bool,
):
    """Create SequencerNodeChart for each service."""
    for service_cfg in deployment_config.services:
        SequencerNodeChart(
            scope=app,
            name=sanitize_name(f"sequencer-{service_cfg.name}"),
            namespace=namespace,
            monitoring=monitoring_enabled,
            common_config=deployment_config.common,
            service_config=service_cfg,
        )


def _create_monitoring_chart(
    app: App,
    namespace: str,
    cluster: str | None,
    monitoring_configs: dict,
):
    """Create MonitoringChart if monitoring is enabled."""
    if not monitoring_configs["enabled"]:
        return

    MonitoringChart(
        scope=app,
        id="sequencer-monitoring",
        cluster=cluster,
        namespace=namespace,
        grafana_dashboard=monitoring_configs["dashboard"],
        grafana_alert_rule_group=monitoring_configs["alerts"],
    )
