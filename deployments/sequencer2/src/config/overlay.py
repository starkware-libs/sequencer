"""Configuration overlay merging with schema-based validation.

This module provides functions to merge base (layout) configurations with overlay
configurations. Overlays can add any key that exists in the Pydantic schema, even if
it's not present in the layout. This makes overlays flexible while still validating
against the schema to prevent invalid keys.
"""

from copy import deepcopy
from typing import Any, Dict, Optional, Union, get_origin

from src.config.schema import ServiceConfig

# Type aliases for dictionary fields that allow arbitrary keys
StrDict = Dict[str, str]
AnyDict = Dict[str, Any]

# Default source identifier when source information is not available
_UNKNOWN_SOURCE = "<unknown overlay>"


def _get_schema_model_for_path(path: str) -> Optional[type[Any]]:
    """Get the Pydantic model class for a given path in the schema.

    Traverses the ServiceConfig schema to find the model for nested paths.
    For example:
    - "persistentVolume" -> PersistentVolume model
    - "persistentVolume.volumeMode" -> PersistentVolume model (returns parent)
    - "service" -> Service model

    Args:
        path: Dot-separated path (e.g., "persistentVolume", "service.ports")

    Returns:
        Pydantic model class if found, None otherwise
    """
    parts = path.split(".")
    current_model: type[Any] = ServiceConfig

    # Traverse the path to find the nested model
    for part in parts:
        if not hasattr(current_model, "model_fields") or part not in current_model.model_fields:
            return None

        field_info = current_model.model_fields[part]
        # Get the annotation (type) of the field
        annotation: Any = field_info.annotation

        # Handle Optional types (e.g., Optional[PersistentVolume])
        if hasattr(annotation, "__origin__"):
            origin = getattr(annotation, "__origin__", None)
            if origin is not None:
                # For Optional[T], get T
                args = getattr(annotation, "__args__", ())
                if args:
                    annotation = args[0]

        # Check if it's a Pydantic model
        if hasattr(annotation, "model_fields"):
            current_model = annotation  # type: ignore[assignment]
        else:
            # Not a model, return None (primitive type or dict)
            return None

    return current_model


def _key_exists_in_schema(key: str, path: str = "") -> bool:
    """Check if a key exists in the ServiceConfig schema.

    Args:
        key: The key to check
        path: Current path in the hierarchy (for nested checks, e.g., "persistentVolume.volumeMode")

    Returns:
        True if key exists in schema, False otherwise
    """
    if not path:
        # Top-level key - check ServiceConfig directly
        return hasattr(ServiceConfig, "model_fields") and key in ServiceConfig.model_fields

    # Nested key - find the parent model
    # Path format: "persistentVolume.volumeMode" -> parent is "persistentVolume"
    if "." in path:
        # Full path provided - extract parent path
        parts = path.rsplit(".", 1)
        parent_path = parts[0]  # e.g., "persistentVolume"
        key_to_check = parts[1] if len(parts) > 1 else key  # e.g., "volumeMode"
    else:
        # Only parent path provided - use the key parameter
        parent_path = path
        key_to_check = key

    parent_model = _get_schema_model_for_path(parent_path) if parent_path else ServiceConfig

    if not parent_model:
        return False

    return hasattr(parent_model, "model_fields") and key_to_check in parent_model.model_fields


def validate_key_exists(
    layout: dict, key: str, current_path: str, source: str, schema_model: Optional[Any] = None
) -> None:
    """Validate that a key exists in either the layout or the schema.

    Args:
        layout: The base layout dictionary.
        key: The key to validate.
        current_path: The full path to the key (for error messages).
        source: The source identifier of the overlay file.
        schema_model: Optional Pydantic model to check against (for nested validation).

    Raises:
        ValueError: If the key does not exist in the layout or schema.
    """
    # First check if key exists in layout
    if key in layout:
        return

    # If not in layout, check if it's valid in the schema
    if schema_model and hasattr(schema_model, "model_fields") and key in schema_model.model_fields:
        return

    # Check against ServiceConfig schema using path
    if _key_exists_in_schema(key, current_path):
        return

    # Key doesn't exist in layout or schema
    raise ValueError(
        f"❌ Overlay file '{source}' tried to add key '{current_path}' "
        f"which is not in the layout or schema"
    )


def _extract_annotation_type(annotation: Any) -> tuple[Any, Any]:
    """Extract the actual type from an annotation, handling Optional/Union.

    Args:
        annotation: The field annotation from Pydantic model.

    Returns:
        Tuple of (annotation, origin) where origin is the type origin (dict, Union, etc.)
    """
    origin = get_origin(annotation)
    if origin is Union:
        args = getattr(annotation, "__args__", ())
        non_none_args = [arg for arg in args if arg is not type(None)]
        if non_none_args:
            annotation = non_none_args[0]
            origin = get_origin(annotation)
    return annotation, origin


def _detect_field_type(parent_model: Any, field_name: str) -> tuple[bool, Optional[Any]]:
    """Detect if a field is a dict type and get its nested model if applicable.

    Args:
        parent_model: The Pydantic model containing the field.
        field_name: Name of the field to check.

    Returns:
        Tuple of (is_dict_field, nested_model):
        - is_dict_field: True if field is a dict type (StrDict/AnyDict)
        - nested_model: Pydantic model if field is a model type, None otherwise
    """
    if not (
        parent_model
        and hasattr(parent_model, "model_fields")
        and field_name in parent_model.model_fields
    ):
        return False, None

    field_info = parent_model.model_fields[field_name]
    annotation, origin = _extract_annotation_type(field_info.annotation)

    if origin is dict:
        return True, None
    elif hasattr(annotation, "model_fields"):
        return False, annotation
    else:
        return False, None


def _merge_dict_field(layout_value: dict, overlay_value: dict, path: str, source: str) -> dict:
    """Merge values when parent is a dict field (allows arbitrary keys).

    Args:
        layout_value: Existing value from layout.
        overlay_value: Value from overlay to merge.
        path: Current path for error messages.
        source: Source identifier.

    Returns:
        Merged dictionary.
    """
    if isinstance(overlay_value, dict):
        existing = layout_value if isinstance(layout_value, dict) else {}
        return _merge_dict_strict(
            existing,
            overlay_value,
            path=path,
            source=source,
            schema_model=None,
            parent_is_dict_field=True,
        )
    return overlay_value


def _merge_nested_dict(
    layout_value: dict,
    overlay_value: dict,
    current_path: str,
    source: str,
    nested_model: Optional[Any],
) -> dict:
    """Merge nested dictionaries with schema validation.

    Args:
        layout_value: Existing nested dict from layout.
        overlay_value: Nested dict from overlay.
        current_path: Current path in hierarchy.
        source: Source identifier.
        nested_model: Pydantic model for nested validation.

    Returns:
        Merged nested dictionary.
    """
    merged_dict = deepcopy(layout_value)

    for overlay_key, overlay_val in overlay_value.items():
        nested_path = f"{current_path}.{overlay_key}"

        # Detect if this nested key is a dict field
        nested_is_dict_field, nested_field_model = _detect_field_type(nested_model, overlay_key)

        # Validate nested key (skip for dict fields)
        if not nested_is_dict_field:
            if nested_model:
                validate_key_exists(
                    merged_dict,
                    overlay_key,
                    nested_path,
                    source,
                    schema_model=nested_model,
                )
            elif not _key_exists_in_schema(overlay_key, nested_path):
                validate_key_exists(merged_dict, overlay_key, nested_path, source)

        # Recursively merge if both are dicts
        if (
            overlay_key in merged_dict
            and isinstance(merged_dict[overlay_key], dict)
            and isinstance(overlay_val, dict)
        ):
            merged_dict[overlay_key] = _merge_dict_strict(
                merged_dict[overlay_key],
                overlay_val,
                path=nested_path,
                source=source,
                schema_model=nested_field_model if nested_field_model else nested_model,
                parent_is_dict_field=nested_is_dict_field,
            )
        else:
            merged_dict[overlay_key] = overlay_val

    return merged_dict


def _merge_dict_strict(
    layout: dict,
    overlay: dict,
    path: str = "",
    source: str = _UNKNOWN_SOURCE,
    schema_model: Optional[Any] = None,
    parent_is_dict_field: bool = False,
) -> dict:
    """Recursively merge overlay dict into layout dict with schema-based validation.

    Keys are validated against the Pydantic schema. If a key exists in the schema,
    it can be added even if not present in the layout. This allows overlays to use
    all schema-valid fields without requiring them to be in the layout.

    Args:
        layout: The base layout dictionary.
        overlay: The overlay dictionary to merge.
        path: The current path in the dictionary hierarchy (for error messages).
        source: The source identifier of the overlay file.
        schema_model: Optional Pydantic model for nested validation.
        parent_is_dict_field: Whether the parent field is a dict type (StrDict/AnyDict).

    Returns:
        A new dictionary with the merged values.

    Raises:
        ValueError: If overlay tries to add a key not in the schema.
    """
    layout_copy = deepcopy(layout)

    for key, val in overlay.items():
        current_path = f"{path}.{key}" if path else key

        # If parent is a dict field, skip all validation
        if parent_is_dict_field:
            existing = layout_copy.get(key, {})
            layout_copy[key] = _merge_dict_field(existing, val, current_path, source)
            continue

        # Get the schema model for this key
        if schema_model is None:
            parent_path = path.rsplit(".", 1)[0] if "." in path else ""
            parent_model = _get_schema_model_for_path(parent_path) if parent_path else ServiceConfig
        else:
            parent_model = schema_model

        # Validate key exists in schema (or layout)
        validate_key_exists(layout, key, current_path, source, schema_model=parent_model)

        # Detect field type (dict field vs model field)
        is_dict_field, nested_model = _detect_field_type(parent_model, key)

        # Handle merging based on value types
        layout_value = layout_copy.get(key)
        if isinstance(layout_value, dict) and isinstance(val, dict):
            if is_dict_field:
                # Dict field: merge without validation
                layout_copy[key] = _merge_dict_strict(
                    layout_value,
                    val,
                    path=current_path,
                    source=source,
                    schema_model=None,
                    parent_is_dict_field=True,
                )
            else:
                # Regular nested dict: merge with schema validation
                layout_copy[key] = _merge_nested_dict(
                    layout_value,
                    val,
                    current_path,
                    source,
                    nested_model,
                )
        else:
            # Simple value: just overwrite
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
    # Filter out services without names and create maps
    layout_map: dict[str, ServiceConfig] = {
        svc.name: svc for svc in layout_services if svc.name is not None
    }
    overlay_map: dict[str, ServiceConfig] = {
        svc.name: svc for svc in overlay_services if svc.name is not None
    }

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
        # Overlay can define common config even if layout doesn't have it
        return deepcopy(overlay_common) if overlay_common else None
    if overlay_common is None:
        return deepcopy(layout_common)

    # At this point both are confirmed to be not None
    # _merge_dict_strict will validate they are dicts
    return _merge_dict_strict(layout_common, overlay_common, path="common", source=source)
