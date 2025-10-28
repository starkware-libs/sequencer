from copy import deepcopy

from .schema import ServiceConfig


def validate_key_exists(layout: dict, key: str, current_path: str, source: str):
    if key not in layout:
        raise AssertionError(f"❌ Overlay file '{source}' tried to add new key '{current_path}'")


def merge_services_layout_with_overlay_strict(
    layout: dict,
    overlay: dict,
    path: str = "",
    source: str = "<unknown>",
) -> dict:
    layout_copy = deepcopy(layout)

    for key, val in overlay.items():
        current_path = f"{path}.{key}" if path else key

        # 1. Ensure key exists in layout
        validate_key_exists(layout, key, current_path, source)

        # 2. Normal dict merge or value overwrite
        if isinstance(layout_copy.get(key), dict) and isinstance(val, dict):
            layout_copy[key] = merge_services_layout_with_overlay_strict(
                layout_copy[key], val, path=current_path, source=source
            )
        else:
            layout_copy[key] = val

    return layout_copy


def apply_services_overlay_strict(
    layout_services: list, overlay_services: list
) -> list:
    merged_services = []
    layout_map = {svc.name: svc for svc in layout_services}
    overlay_map = {svc.name: svc for svc in overlay_services}

    for svc_name in overlay_map:
        if svc_name not in layout_map:
            raise AssertionError(
                f"❌ Overlay tried to introduce new service '{svc_name}' not in layout"
            )

    for svc_name, svc in layout_map.items():
        if svc_name in overlay_map:
            overlay_service = overlay_map[svc_name]
            source = getattr(overlay_service, "_source", "<unknown overlay>")

            merged = merge_services_layout_with_overlay_strict(
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
    layout_common: dict,
    overlay_common: dict,
    source: str = "<unknown overlay>",
) -> dict:
    layout_common_copy = deepcopy(layout_common)

    for key, val in overlay_common.items():
        if key not in layout_common:
            raise AssertionError(
                f"❌ Overlay file '{source}' tried to add new key 'common.{key}' not present in layout"
            )

        if isinstance(val, dict) and isinstance(layout_common.get(key), dict):
            layout_common_copy[key] = merge_common_with_overlay_strict(
                layout_common[key], val, source=source
            )
        else:
            layout_common_copy[key] = val

    return layout_common_copy
