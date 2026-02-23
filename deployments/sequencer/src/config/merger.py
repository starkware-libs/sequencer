from src.config.loaders import DeploymentConfigLoader
from src.config.overlay import apply_services_overlay_strict
from src.config.schema import DeploymentConfig as DeploymentSchema
from src.config.schema import ServiceConfig


def merge_configs(
    layout_common_config_path: str | None,
    layout_services_config_dir_path: str,
    overlay_layers: list[tuple[str | None, str | None]] | None = None,
) -> DeploymentSchema:
    """
    Merge base (layout) configs with optional overlay layers.

    Merge pipeline:
    1. Commons chain: layout_common <- overlay1_common <- overlay2_common -> merged_common
    2. Services chain: layout_services <- overlay1_services <- overlay2_services -> merged_services
    3. merged_common merged into each merged_service -> final_services

    Each overlay layer's common.yaml and services/ are optional; if absent that layer
    is skipped for that chain. Uses DeploymentConfigLoader for loading and validation.
    Returns a validated DeploymentConfig schema object.
    """
    overlay_layers = overlay_layers or []

    # --- Load layout configs ---
    layout_loader = DeploymentConfigLoader(
        configs_dir_path=layout_services_config_dir_path,
        config_base_dir=config_base_dir,
    )
    layout_common = layout_loader._load_common_config()
    layout_services = layout_loader._load_service_configs_from_dir()

    merged_common = layout_common
    merged_services = layout_services

    # --- Apply each overlay layer in order (left-to-right, last wins) ---
    for overlay_common_path, overlay_services_path in overlay_layers:
        if overlay_services_path:
            overlay_loader = DeploymentConfigLoader(
                configs_dir_path=overlay_services_path,
                common_config_path=overlay_common_path,
            )
            overlay_services = overlay_loader._load_service_configs_from_dir()
            merged_services = apply_services_overlay_strict(
                merged_services, overlay_services
            )
        if overlay_common_path:
            overlay_loader = DeploymentConfigLoader(
                configs_dir_path=overlay_services_path or layout_services_config_dir_path,
                common_config_path=overlay_common_path,
            )
            overlay_common = overlay_loader._load_common_config()
            merged_common = merge_common_with_overlay_strict(
                merged_common, overlay_common
            )

    # --- Merge common into each service (once at the end) ---
    final_services = [
        _merge_common_into_service(merged_common, service) for service in merged_services
    ]

    merged = {
        "common": ServiceConfig(),
        "services": merged_services,
    }
    return DeploymentSchema.model_validate(merged)
