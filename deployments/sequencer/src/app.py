import sys
from pathlib import Path

from cdk8s import App, YamlOutputType
from src.charts.monitoring import MonitoringChart
from src.charts.node import SequencerNodeChart
from src.cli import argument_parser
from src.config.loaders import (
    ConfigValidationError,
    GrafanaAlertRuleGroupConfigLoader,
    GrafanaDashboardConfigLoader,
)
from src.config.merger import merge_configs
from src.config.schema import DeploymentConfig as DeploymentSchema
from src.config.schema import Image
from src.utils import sanitize_name


def main():
    """Main entry point for CDK8s application."""
    args = argument_parser()
    _validate_args(args)

    app = App(yaml_output_type=YamlOutputType.FOLDER_PER_CHART_FILE_PER_RESOURCE)
    base_dir = _get_base_dir()

    # Load configuration
    deployment_config = _load_deployment_config(base_dir, args.layout, args.overlay or [])

    # Override image if provided
    if args.image:
        _override_image_for_all_services(deployment_config, args.image)

    # Prepare monitoring
    monitoring_configs = _prepare_monitoring_configs(args)
    namespace = sanitize_name(args.namespace)

    try:
        # Create charts
        _create_service_charts(
            app,
            deployment_config,
            namespace,
            monitoring_configs["enabled"],
            args.layout,
            args.overlay or [],
        )
        _create_monitoring_chart(app, namespace, args.cluster, monitoring_configs)

        # Synthesize manifests
        app.synth()
    except ConfigValidationError:
        # For config validation errors, suppress traceback unless verbose mode is enabled
        if args.verbose:
            # Re-raise with full traceback in verbose mode
            raise
        else:
            # Exit cleanly without traceback
            sys.exit(1)


def _validate_args(args):
    """Validate CLI arguments."""
    if args.monitoring_dashboard_file and not args.cluster:
        raise ValueError("--cluster is required when --monitoring-dashboard-file is provided.")


def _get_base_dir() -> Path:
    """Resolve base directory relative to main.py."""
    return Path(__file__).resolve().parents[1]


def _get_config_paths(
    base_dir: Path, layout: str, overlays: list[str]
) -> tuple[Path | None, Path, list[tuple[Path | None, Path | None]]]:
    """Get layout and overlay config paths.

    Returns (layout_common, layout_services, overlay_layers) where overlay_layers
    is a list of (common_path, services_path) per overlay. Each path is optional
    (None if file/dir does not exist).

    Overlay paths must start with the layout name and use dot notation:
    - 'hybrid.mainnet' -> configs/overlays/hybrid/mainnet/
    - 'hybrid.mainnet.apollo-mainnet-0' -> configs/overlays/hybrid/mainnet/apollo-mainnet-0/
    """
    layout_node_dir = base_dir / "configs" / "layouts" / layout
    layout_services = layout_node_dir / "services"

    # common.yaml is optional in layout
    layout_common_path = base_dir / "configs" / "layouts" / layout / "common.yaml"
    layout_common = layout_common_path if layout_common_path.exists() else None

    overlay_layers: list[tuple[Path | None, Path | None]] = []
    for overlay in overlays:
        overlay_path_segments = overlay.split(".")

        if not overlay_path_segments or overlay_path_segments[0] != layout:
            raise ValueError(
                f"Overlay path '{overlay}' must start with the layout name '{layout}'. "
                f"Example: '{layout}.mainnet.apollo-mainnet-0'"
            )

        overlay_base_path = base_dir / "configs" / "overlays" / layout
        for segment in overlay_path_segments[1:]:
            overlay_base_path = overlay_base_path / segment

        overlay_common_path = overlay_base_path / "common.yaml"
        overlay_common = overlay_common_path if overlay_common_path.exists() else None
        overlay_services_path = overlay_base_path / "services"
        overlay_services = overlay_services_path if overlay_services_path.is_dir() else None
        overlay_layers.append((overlay_common, overlay_services))

    return (layout_common, layout_services, overlay_layers)


def _load_deployment_config(base_dir: Path, layout: str, overlays: list[str]) -> DeploymentSchema:
    """Load and merge deployment configuration."""
    layout_common, layout_services, overlay_layers = _get_config_paths(base_dir, layout, overlays)

    overlay_layers_str: list[tuple[str | None, str | None]] = [
        (
            str(common) if common else None,
            str(services) if services else None,
        )
        for common, services in overlay_layers
    ]
    return merge_configs(
        layout_common_config_path=str(layout_common) if layout_common else None,
        layout_services_config_dir_path=str(layout_services),
        overlay_layers=overlay_layers_str,
    )


def _parse_image(image_str: str) -> Image:
    """Parse image string into Image object.

    Supports formats:
    - 'repository:tag' -> Image(repository='repository', tag='tag')
    - 'repository' -> Image(repository='repository', tag='latest')
    """
    if ":" in image_str:
        repository, tag = image_str.rsplit(":", 1)
        return Image(repository=repository, tag=tag)
    else:
        return Image(repository=image_str, tag="latest")


def _override_image_for_all_services(deployment_config: DeploymentSchema, image_str: str):
    """Override image for all services in the deployment config."""
    base_image = _parse_image(image_str)

    # Override image for each service
    for service_cfg in deployment_config.services:
        # Preserve imagePullPolicy if it exists
        image_pull_policy = service_cfg.image.imagePullPolicy if service_cfg.image else None

        # Create a new Image instance for each service
        service_cfg.image = Image(
            repository=base_image.repository,
            tag=base_image.tag,
            digest=base_image.digest,
            imagePullPolicy=image_pull_policy or base_image.imagePullPolicy,
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
    layout: str,
    overlays: list[str],
):
    """Create SequencerNodeChart for each service."""
    for service_cfg in deployment_config.services:
        SequencerNodeChart(
            scope=app,
            name=sanitize_name(f"sequencer-{service_cfg.name}"),
            namespace=namespace,
            monitoring=monitoring_enabled,
            service_config=service_cfg,
            layout=layout,
            overlays=overlays,
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
