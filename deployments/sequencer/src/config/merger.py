from src.config.loaders import DeploymentConfigLoader
from src.config.overlay import (
    apply_services_overlay_strict,
    merge_common_with_overlay_strict,
)
from src.config.schema import DeploymentConfig as DeploymentSchema
from src.config.schema import (
    ServiceConfig,
)


def merge_configs(
    layout_shared_config_paths: list[str],
    layout_services_config_dir_path: str,
    overlay_shared_config_paths: list[list[str]] | None = None,
    overlay_services_config_dir_paths: list[str] | None = None,
    config_base_dir: str | None = None,
) -> DeploymentSchema:
    """
    Merge base (layout) configs with optional overlay configs.

    Overlays are applied in order: layout, then overlay_0, overlay_1, etc. (each element
    in the overlays list). Later overlays override earlier ones.

    Merge pipeline:
    1. Layout shared config + each overlay's shared config → merged_common (for schema "common" key).
    2. Layout services + each overlay's service configs → merged_services. Common config is merged into
       each service via the include mechanism (shared config paths are prepended to each service file's
       include list), so no separate merge step is needed.

    Uses DeploymentConfigLoader's internal YAML loading and validation logic.
    Returns a validated DeploymentConfig schema object.
    """
    overlay_shared_paths = overlay_shared_config_paths or []
    overlay_services_paths = overlay_services_config_dir_paths or []
    if len(overlay_shared_paths) != len(overlay_services_paths):
        raise ValueError(
            "overlay_shared_config_paths and overlay_services_config_dir_paths must have the same length"
        )

    # --- Load layout configs using DeploymentConfigLoader ---
    layout_loader = DeploymentConfigLoader(
        configs_dir_path=layout_services_config_dir_path,
        shared_config_paths=layout_shared_config_paths or None,
        config_base_dir=config_base_dir,
    )

    layout_common = layout_loader._load_common_config()
    layout_services = layout_loader._load_service_configs_from_dir()

    merged_common = layout_common
    merged_services = layout_services

    # --- Load and apply each overlay in order ---
    # Common config is referenced via 'include' in each service YAML (not merged here).
    for overlay_paths, overlay_services_path in zip(overlay_shared_paths, overlay_services_paths):
        overlay_loader = DeploymentConfigLoader(
            configs_dir_path=overlay_services_path,
            shared_config_paths=overlay_paths or None,
            config_base_dir=config_base_dir,
        )
        overlay_common = overlay_loader._load_common_config()
        overlay_services = overlay_loader._load_service_configs_from_dir()

        merged_services = apply_services_overlay_strict(merged_services, overlay_services)
        merged_common = merge_common_with_overlay_strict(merged_common, overlay_common)

    # Services already have common merged in via 'include' in each service YAML.
    final_services = merged_services

    # --- Combine into a validated Deployment schema ---
    # Use default ServiceConfig() if merged_common is None (shared config is optional)
    merged = {
        "common": merged_common if merged_common is not None else ServiceConfig(),
        "services": final_services,
    }

    return DeploymentSchema.model_validate(merged)
