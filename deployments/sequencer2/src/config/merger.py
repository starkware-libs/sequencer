from .schema import DeploymentConfig as DeploymentSchema
from .deployment import DeploymentConfig
from .overlay import (
    apply_services_overlay_strict,
    merge_common_with_overlay_strict,
)


def merge_configs(
    layout_common_config_path: str,
    layout_services_config_dir_path: str,
    overlay_common_config_path: str | None = None,
    overlay_services_config_dir_path: str | None = None,
) -> DeploymentSchema:
    """
    Merge base (layout) configs with optional overlay configs.

    Uses DeploymentConfigs internal YAML loading and validation logic.
    Returns a validated DeploymentConfig schema object.
    """

    # --- Load layout configs using DeploymentConfig ---
    layout_loader = DeploymentConfig(
        configs_dir_path=layout_services_config_dir_path,
        common_config_path=layout_common_config_path,
    )

    layout_common = layout_loader._load_common_config()
    layout_services = layout_loader._load_service_configs_from_dir()

    merged_common = layout_common
    merged_services = layout_services

    # --- Load overlay configs (if provided) ---
    if overlay_services_config_dir_path:
        overlay_loader = DeploymentConfig(
            configs_dir_path=overlay_services_config_dir_path,
            common_config_path=overlay_common_config_path,
        )
        overlay_common = overlay_loader._load_common_config()
        overlay_services = overlay_loader._load_service_configs_from_dir()

        # Merge services + common parts using strict overlay semantics
        merged_services = apply_services_overlay_strict(layout_services, overlay_services)
        merged_common = merge_common_with_overlay_strict(layout_common, overlay_common)

    # --- Combine into a validated Deployment schema ---
    merged = {
        "common": merged_common,
        "services": merged_services,
    }

    return DeploymentSchema.model_validate(merged)
