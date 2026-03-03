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

# Single entry filename at each layout/overlay node. That file may use optional "include: path" to pull in other YAMLs.
ENTRY_FILENAME = "common.yaml"


def _entry_path(node_dir: Path) -> Path | None:
    """Return path to the entry file at this node if it exists, else None."""
    p = node_dir / ENTRY_FILENAME
    return p if p.is_file() else None


def main():
    """Main entry point for CDK8s application."""
    args = argument_parser()
    _validate_args(args)

    app = App(yaml_output_type=YamlOutputType.FOLDER_PER_CHART_FILE_PER_RESOURCE)
    base_dir = _get_base_dir()

    # Load configuration
    deployment_config = _load_deployment_config(base_dir, args.layout, args.overlays)

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
            args.overlays,
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
) -> tuple[list[Path], Path, list[tuple[list[Path], Path]]]:
    """Get layout and overlay config paths.

    Each overlay in the overlays list must start with the layout name and use dot notation
    for nested paths:
    - 'hybrid.sepolia-integration.node-01' -> configs/overlays/hybrid/sepolia-integration/node-01/
    - 'hybrid.sepolia-integration.node-02' -> configs/overlays/hybrid/sepolia-integration/node-02/

    At each node (layout or overlay), shared values come from the single entry file (if present).
    That file may have an optional "include: path" key pointing to another YAML file; the loader
    resolves includes recursively and merges (included as base, current on top).

    Returns:
        (layout_shared_paths, layout_services, overlay_paths) where:
        - layout_shared_paths: list of 0 or 1 path (entry file at layout node).
        - layout_services: path to the layout's services directory.
        - overlay_paths: per overlay, (list of 0 or 1 entry path, services dir path).
    """
    layout_node_dir = base_dir / "configs" / "layouts" / layout
    layout_services = layout_node_dir / "services"
    layout_entry = _entry_path(layout_node_dir)
    layout_shared_paths = [layout_entry] if layout_entry else []

    overlay_paths: list[tuple[list[Path], Path]] = []
    for overlay in overlays:
        overlay_path_segments = overlay.split(".")
        if not overlay_path_segments or overlay_path_segments[0] != layout:
            raise ValueError(
                f"Overlay path '{overlay}' must start with the layout name '{layout}'. "
                f"Example: '{layout}.sepolia-integration.node-01'"
            )

        overlay_base_path = base_dir / "configs" / "overlays" / layout
        for segment in overlay_path_segments[1:]:
            overlay_base_path = overlay_base_path / segment

        overlay_services = overlay_base_path / "services"
        overlay_entry = _entry_path(overlay_base_path)
        overlay_shared_paths = [overlay_entry] if overlay_entry else []
        overlay_paths.append((overlay_shared_paths, overlay_services))

    return (layout_shared_paths, layout_services, overlay_paths)


def _load_deployment_config(base_dir: Path, layout: str, overlays: list[str]) -> DeploymentSchema:
    """Load and merge deployment configuration."""
    layout_shared_paths, layout_services, overlay_paths = _get_config_paths(
        base_dir, layout, overlays
    )

    layout_shared_config_paths = [str(p) for p in layout_shared_paths]
    overlay_shared_config_paths = [
        [str(p) for p in shared_list] for shared_list, _ in overlay_paths
    ]
    overlay_services_config_dir_paths = [str(svc_path) for _, svc_path in overlay_paths]

    return merge_configs(
        layout_shared_config_paths=layout_shared_config_paths,
        layout_services_config_dir_path=str(layout_services),
        overlay_shared_config_paths=overlay_shared_config_paths,
        overlay_services_config_dir_paths=overlay_services_config_dir_paths,
        config_base_dir=str(base_dir),
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
