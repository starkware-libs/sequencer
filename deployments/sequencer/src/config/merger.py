from src.config.loaders import DeploymentConfigLoader
from src.config.overlay import apply_services_overlay_strict
from src.config.schema import DeploymentConfig as DeploymentSchema
from src.config.schema import ServiceConfig


def merge_configs(
    layout_services_config_dir_path: str,
    overlay_services_config_dir_paths: list[str],
    config_base_dir: str,
) -> DeploymentSchema:
    """
    Merge base (layout) configs with required overlay configs.

    Overlays are applied in order: layout, then overlay_0, overlay_1, etc. (each element
    in the overlays list). Later overlays override earlier ones.

    There is no separate node-level common file; each service YAML uses 'include' to pull
    in whatever shared files it needs. The deployment schema's "common" key is left as
    default (empty ServiceConfig).
    """
    # --- Load layout services ---
    layout_loader = DeploymentConfigLoader(
        configs_dir_path=layout_services_config_dir_path,
        config_base_dir=config_base_dir,
    )
    merged_services = layout_loader._load_service_configs_from_dir()

    # --- Load and apply each overlay in order ---
    for overlay_services_path in overlay_services_config_dir_paths:
        overlay_loader = DeploymentConfigLoader(
            configs_dir_path=overlay_services_path,
            config_base_dir=config_base_dir,
        )
        overlay_services = overlay_loader._load_service_configs_from_dir()
        merged_services = apply_services_overlay_strict(merged_services, overlay_services)

    merged = {
        "common": ServiceConfig(),
        "services": merged_services,
    }
    return DeploymentSchema.model_validate(merged)
