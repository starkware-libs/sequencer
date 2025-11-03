"""Configuration overlay merging with strict validation.

This module provides functions to merge base (layout) configurations with overlay
configurations. Overlays can only modify existing keys - they cannot add new keys
or introduce new services. This ensures that overlays are truly "overlays" that
customize existing configurations rather than creating new ones.
"""

from copy import deepcopy
from typing import Any

from src.config.schema import ServiceConfig

# Default source identifier when source information is not available
_UNKNOWN_SOURCE = "<unknown overlay>"


def validate_key_exists(layout: dict, key: str, current_path: str, source: str) -> None:
    """Validate that a key exists in the layout dictionary.

    Args:
        layout: The base layout dictionary.
        key: The key to validate.
        current_path: The full path to the key (for error messages).
        source: The source identifier of the overlay file.

    Raises:
        ValueError: If the key does not exist in the layout.
    """
    if key not in layout:
        raise ValueError(f"❌ Overlay file '{source}' tried to add new key '{current_path}'")


def _merge_dict_strict(
    layout: dict,
    overlay: dict,
    path: str = "",
    source: str = _UNKNOWN_SOURCE,
) -> dict:
    """Recursively merge overlay dict into layout dict with strict validation.

    Only existing keys in the layout can be modified. New keys cannot be added.

    Args:
        layout: The base layout dictionary.
        overlay: The overlay dictionary to merge.
        path: The current path in the dictionary hierarchy (for error messages).
        source: The source identifier of the overlay file.

    Returns:
        A new dictionary with the merged values.

    Raises:
        ValueError: If overlay tries to add a new key not present in layout.
    """
    layout_copy = deepcopy(layout)

    for key, val in overlay.items():
        current_path = f"{path}.{key}" if path else key

        # Ensure key exists in layout
        validate_key_exists(layout, key, current_path, source)

        # Recursive merge for nested dicts, otherwise overwrite
        if isinstance(layout_copy.get(key), dict) and isinstance(val, dict):
            layout_copy[key] = _merge_dict_strict(
                layout_copy[key], val, path=current_path, source=source
            )
        else:
            layout_copy[key] = val

    return layout_copy


def merge_service_overlay(
    layout: dict,
    overlay: dict,
    path: str = "",
    source: str = _UNKNOWN_SOURCE,
) -> dict:
    """Merge service overlay into layout dictionary.

    This is a wrapper around `_merge_dict_strict` with service-specific naming.

    Args:
        layout: The base service layout dictionary.
        overlay: The service overlay dictionary to merge.
        path: The current path in the dictionary hierarchy (for error messages).
        source: The source identifier of the overlay file.

    Returns:
        A new dictionary with the merged values.
    """
    return _merge_dict_strict(layout, overlay, path=path, source=source)


def apply_services_overlay_strict(
    layout_services: list[ServiceConfig],
    overlay_services: list[ServiceConfig],
) -> list[ServiceConfig]:
    """Apply overlay configurations to layout services with strict validation.

    Overlay services can only modify existing services - they cannot introduce
    new services not present in the layout.

    Args:
        layout_services: List of base service configurations.
        overlay_services: List of overlay service configurations.

    Returns:
        A list of merged service configurations.

    Raises:
        ValueError: If overlay tries to introduce a new service not in layout.
    """
    merged_services: list[ServiceConfig] = []
    layout_map = {svc.name: svc for svc in layout_services}
    overlay_map = {svc.name: svc for svc in overlay_services}

    # Validate that overlay doesn't introduce new services
    for svc_name in overlay_map:
        if svc_name not in layout_map:
            raise ValueError(
                f"❌ Overlay tried to introduce new service '{svc_name}' not in layout"
            )

    # Merge services
    for svc_name, svc in layout_map.items():
        if svc_name in overlay_map:
            overlay_service = overlay_map[svc_name]
            source = getattr(overlay_service, "_source", _UNKNOWN_SOURCE)

            merged = merge_service_overlay(
                svc.model_dump(exclude_none=True),  # base dict
                overlay_service.model_dump(exclude_unset=True, exclude_none=True),
                path=svc_name,
                source=source,
            )
            merged_services.append(ServiceConfig.model_validate(merged))
        else:
            merged_services.append(svc)

    return merged_services


def merge_common_with_overlay_strict(
    layout_common: dict[str, Any] | None,
    overlay_common: dict[str, Any] | None,
    source: str = _UNKNOWN_SOURCE,
) -> dict[str, Any] | None:
    """Merge common overlay configuration into layout common configuration.

    Args:
        layout_common: The base common configuration dictionary.
        overlay_common: The overlay common configuration dictionary.
        source: The source identifier of the overlay file.

    Returns:
        A new dictionary with the merged values, or None if both inputs are None.

    Raises:
        ValueError: If overlay tries to add a new key not present in layout.
        TypeError: If inputs are not dictionaries or None.
    """
    # Handle None cases
    if layout_common is None and overlay_common is None:
        return None
    if layout_common is None:
        raise ValueError(f"Layout common config is None but overlay exists: {source}")
    if overlay_common is None:
        return deepcopy(layout_common)

    # At this point both are confirmed to be not None
    # _merge_dict_strict will validate they are dicts
    return _merge_dict_strict(layout_common, overlay_common, path="common", source=source)
