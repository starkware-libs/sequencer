from copy import deepcopy

from src.config.loaders import DeploymentConfigLoader
from src.config.overlay import (
    apply_services_overlay_strict,
    merge_common_with_overlay_strict,
)
from src.config.schema import (
    CommonConfig,
)
from src.config.schema import DeploymentConfig as DeploymentSchema
from src.config.schema import (
    ServiceConfig,
)


def _merge_common_into_service(
    common_config: CommonConfig | dict | None, service_config: ServiceConfig
) -> ServiceConfig:
    """Merge common config fields into service config.

    This automatically merges ANY field from common.yaml that exists in ServiceConfig schema.
    No special code needed per field - just add the field to CommonConfig schema and it works.

    Special handling:
    - service.ports: Merges lists by name (service ports override common ports with same name)
    - All other fields: Deep merge (common first, then service overrides)

    Args:
        common_config: The merged common configuration (dict, CommonConfig, or None if no common.yaml exists)
        service_config: The service configuration (after overlay merging)

    Returns:
        A new ServiceConfig with common config merged in
    """
    # Start with service config as base
    service_dict = service_config.model_dump(mode="python", exclude_unset=True, exclude_none=True)

    # Handle None, dict, and CommonConfig types
    if common_config is None:
        return service_config  # No common config to merge

    if isinstance(common_config, dict):
        common_dict = common_config
    else:
        # Use exclude_unset=True to avoid including fields with default_factory that weren't explicitly set
        common_dict = (
            common_config.model_dump(mode="python", exclude_unset=True, exclude_none=True)
            if common_config
            else {}
        )

    if not common_dict:
        return service_config

    # Get fields that exist in both CommonConfig and ServiceConfig schemas
    common_fields = set(CommonConfig.model_fields.keys())
    service_fields = set(ServiceConfig.model_fields.keys())
    mergeable_fields = common_fields & service_fields

    # Exclude fields that shouldn't be merged from common config
    # name: Service names are service-specific and should never be overridden by common config
    exclude_fields = {"name"}
    mergeable_fields = mergeable_fields - exclude_fields

    # Deep merge function for nested dictionaries
    def deep_merge(base: dict, overlay: dict) -> dict:
        """Recursively merge overlay into base.

        Semantics:
        - Start with base (service config) to preserve all service-specific fields
        - Overlay (common config) can add new fields or merge nested dicts
        - For non-dict values, base (service) takes precedence (overlay doesn't override)
        """
        result = deepcopy(base)
        for key, value in overlay.items():
            if key in result and isinstance(result[key], dict) and isinstance(value, dict):
                # Both are dicts - recursively merge (overlay into base)
                result[key] = deep_merge(result[key], value)
            elif key not in result:
                # Key not in base - add it from overlay (common can add fields)
                result[key] = deepcopy(value)
            # If key exists in base and at least one is not a dict, base (service) takes precedence
            # Don't override service values with common values
        return result

    # Special merge function for service.ports (list merge by name)
    def merge_service_ports(common_ports: list, service_ports: list) -> list:
        """Merge service ports by name - service ports override common ports with same name."""
        if not common_ports:
            return service_ports
        if not service_ports:
            return common_ports

        # Build port dict by name for merging (only need common_ports_dict for lookup)
        common_ports_dict = {p["name"]: p for p in common_ports if p.get("name")}

        # Convert back to list, preserving service port order first
        merged_ports = []
        processed_names = set()

        # Add service ports first (preserve order)
        for p in service_ports:
            if p.get("name"):
                processed_names.add(p["name"])
                merged_ports.append(p)
            else:
                # Ports without names are added as-is
                merged_ports.append(p)

        # Add remaining common ports
        for port_name, port_dict in common_ports_dict.items():
            if port_name not in processed_names:
                merged_ports.append(port_dict)

        return merged_ports

    # Merge each mergeable field
    for field_name in mergeable_fields:
        if field_name not in common_dict:
            continue  # Field not present in common config, skip

        common_value = common_dict[field_name]
        service_value = service_dict.get(field_name)

        # Special handling for service.ports
        if field_name == "service" and isinstance(common_value, dict) and common_value.get("ports"):
            if "service" not in service_dict:
                service_dict["service"] = {}
            service_ports = service_dict["service"].get("ports", [])
            merged_ports = merge_service_ports(common_value["ports"], service_ports)
            # Merge the rest of service config (if any)
            # Start with service config as base to preserve all service-specific fields
            if service_value:
                merged_service = deep_merge(service_value, common_value)
                merged_service["ports"] = merged_ports
                service_dict["service"] = merged_service
            else:
                service_dict["service"] = {**common_value, "ports": merged_ports}
        # Special handling for config.sequencerConfig (nested merge)
        elif field_name == "config" and isinstance(common_value, dict):
            # Start with service config as base to preserve all service-specific fields (like configList)
            if service_value:
                merged_config = deepcopy(service_value)
            else:
                merged_config = {}

            # Merge common config into service config (common can add or modify fields)
            # For sequencerConfig, merge nested dicts
            if common_value.get("sequencerConfig"):
                if "sequencerConfig" not in merged_config:
                    merged_config["sequencerConfig"] = {}
                # Merge: common first, then service (service overrides common)
                merged_seq_config = deepcopy(common_value["sequencerConfig"])
                merged_seq_config.update(merged_config["sequencerConfig"])
                merged_config["sequencerConfig"] = merged_seq_config

            # For all other fields in common config, merge them in
            for key, value in common_value.items():
                if key == "sequencerConfig":
                    # Already handled above
                    continue
                if key not in merged_config:
                    # Field not in service config, add it from common
                    merged_config[key] = deepcopy(value)
                elif isinstance(merged_config[key], dict) and isinstance(value, dict):
                    # Both are dicts, recursively merge (common into service)
                    merged_config[key] = deep_merge(merged_config[key], value)
                # If service has a non-dict value, it takes precedence (don't override)

            service_dict["config"] = merged_config
        # Generic deep merge for all other fields
        else:
            if service_value is None:
                # Service doesn't have this field, use common
                service_dict[field_name] = common_value
            elif isinstance(common_value, list) and isinstance(service_value, list):
                # Merge lists: common first, then service (service items appended)
                # If service list is empty, use common list
                if not service_value:
                    service_dict[field_name] = common_value
                else:
                    # Merge: common items first, then service items
                    # For lists of dicts (like env), we might want to deduplicate by a key
                    # For now, just append service to common
                    merged_list = list(common_value) + list(service_value)
                    service_dict[field_name] = merged_list
            elif isinstance(common_value, dict) and isinstance(service_value, dict):
                # Check if this is an "enabled" field (like podMonitoring, networkPolicy, etc.)
                # and if service has enabled=False with no other meaningful config, treat as default
                def is_default_enabled_field(value_dict: dict) -> bool:
                    """Check if an enabled field is essentially a default/empty configuration.

                    A field is considered default if:
                    - enabled=False (or not set, defaults to False)
                    - No custom name, annotations, or labels
                    - No meaningful spec content (custom selectors, limits, etc.)
                    - Note: Default endpoints in spec are OK - they're just schema defaults
                    """
                    if not value_dict:
                        return True
                    # If enabled is explicitly True, it's not a default
                    if value_dict.get("enabled") is True:
                        return False
                    # If enabled is False or None (defaults to False), check for meaningful content
                    # Check top-level fields
                    if (
                        value_dict.get("name")
                        or value_dict.get("annotations")
                        or value_dict.get("labels")
                    ):
                        return False
                    # Check nested spec (for podMonitoring, etc.)
                    spec = value_dict.get("spec")
                    if spec and isinstance(spec, dict):
                        # If spec has meaningful custom content, it's not a default
                        selector = spec.get("selector", {})
                        if (
                            selector.get("matchLabels")
                            or selector.get("matchExpressions")
                            or spec.get("filterRunning") is not None
                            or spec.get("limits")
                            or spec.get("targetLabels")
                        ):
                            return False
                        # Endpoints with meaningful custom content (not just defaults)
                        endpoints = spec.get("endpoints", [])
                        if endpoints:
                            # Check if endpoints have custom content beyond defaults
                            for ep in endpoints:
                                if isinstance(ep, dict):
                                    # If endpoint has custom path, interval, or other non-default values
                                    # (beyond what might be in common), it's meaningful
                                    # But we can't easily detect this, so if there are endpoints
                                    # and enabled=False, we'll merge to be safe
                                    # Actually, if enabled=False, even with endpoints, it's likely a default
                                    # The endpoints might be from schema defaults
                                    pass
                    # If we get here and enabled is False/None, it's likely a default
                    return value_dict.get("enabled") is False or value_dict.get("enabled") is None

                if is_default_enabled_field(service_value):
                    # Service has essentially a default/empty config, use common instead
                    service_dict[field_name] = common_value
                else:
                    # Both are dicts - start with service (base) and overlay common (common can add/modify)
                    # This preserves all service-specific fields
                    service_dict[field_name] = deep_merge(service_value, common_value)
            else:
                # Service has a value - it overrides common (for non-dict types)
                service_dict[field_name] = service_value

    # Validate and return new ServiceConfig
    return ServiceConfig.model_validate(service_dict)


def merge_configs(
    layout_common_config_path: str,
    layout_services_config_dir_path: str,
    overlay_common_config_path: str | None = None,
    overlay_services_config_dir_path: str | None = None,
) -> DeploymentSchema:
    """
    Merge base (layout) configs with optional overlay configs.

    Merge pipeline:
    1. Layout common.yaml + Overlay common.yaml → merged_common
    2. Layout service.yaml + Overlay service.yaml → merged_service
    3. merged_common + merged_service → final_service_config (common merged into service)

    Uses DeploymentConfigLoader's internal YAML loading and validation logic.
    Returns a validated DeploymentConfig schema object.
    """

    # --- Load layout configs using DeploymentConfigLoader ---
    layout_loader = DeploymentConfigLoader(
        configs_dir_path=layout_services_config_dir_path,
        common_config_path=layout_common_config_path,
    )

    layout_common = layout_loader._load_common_config()
    layout_services = layout_loader._load_service_configs_from_dir()

    merged_common = layout_common
    merged_services = layout_services

    # --- Load overlay configs (if provided) ---
    if overlay_services_config_dir_path:
        overlay_loader = DeploymentConfigLoader(
            configs_dir_path=overlay_services_config_dir_path,
            common_config_path=overlay_common_config_path,
        )
        overlay_common = overlay_loader._load_common_config()
        overlay_services = overlay_loader._load_service_configs_from_dir()

        # Merge services + common parts using strict overlay semantics
        merged_services = apply_services_overlay_strict(layout_services, overlay_services)
        merged_common = merge_common_with_overlay_strict(layout_common, overlay_common)

    # --- Merge common config into each service config ---
    # This ensures constructs only need to check service_config, not both
    # merged_common can be None if no common.yaml exists in layout or overlay
    final_services = [
        _merge_common_into_service(merged_common, service) for service in merged_services
    ]

    # --- Combine into a validated Deployment schema ---
    # Use default ServiceConfig() if merged_common is None (common.yaml is optional)
    merged = {
        "common": merged_common if merged_common is not None else ServiceConfig(),
        "services": final_services,
    }

    return DeploymentSchema.model_validate(merged)
