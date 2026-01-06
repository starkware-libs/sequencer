"""
Shared module for loading and merging sequencer configuration files.

This module provides utilities for loading YAML configs, merging layout and overlay
configurations, and finding workspace roots. Used by system test scripts.
"""
from copy import deepcopy
from pathlib import Path
from typing import Any, Dict, List, Optional

import yaml


def load_yaml(file_path: Path) -> Dict[str, Any]:
    """Load a YAML file."""
    if not file_path.exists():
        return {}
    with open(file_path, "r", encoding="utf-8") as f:
        return yaml.safe_load(f) or {}


def deep_merge_dict(base: Dict[str, Any], overlay: Dict[str, Any]) -> Dict[str, Any]:
    """Deep merge overlay dict into base dict."""
    result = deepcopy(base)
    for key, value in overlay.items():
        if key in result and isinstance(result[key], dict) and isinstance(value, dict):
            result[key] = deep_merge_dict(result[key], value)
        else:
            result[key] = value
    return result


def find_workspace_root() -> Optional[str]:
    """
    Auto-detect workspace root: ../.. from script location.

    Script is at: scripts/system_tests/*.py
    Repo root is: ../.. from script location
    """
    script_dir = Path(__file__).parent.resolve()
    workspace_root = script_dir.parent.parent.resolve()
    return str(workspace_root)


def load_and_merge_configs(
    workspace: str, layout: str, overlay: Optional[str] = None
) -> List[Dict[str, Any]]:
    """
    Load and merge sequencer configs (layout + overlay if provided).

    Merge order to match deployment system:
    1. Layout service + Overlay service → merged service (overlay service overrides layout service)
    2. Layout common + Overlay common → merged common (overlay common overrides layout common)
    3. merged common merged into merged service → final config

    Args:
        workspace: Workspace root directory
        layout: Layout name (e.g., 'hybrid')
        overlay: Optional overlay path in dot notation (e.g., 'hybrid.testing.node-0')

    Returns:
        List of merged service configs.
    """
    base_dir = Path(workspace) / "deployments" / "sequencer"

    # Load layout common.yaml
    layout_common_path = base_dir / "configs" / "layouts" / layout / "common.yaml"
    layout_common = load_yaml(layout_common_path)

    # Load layout service configs
    layout_services_dir = base_dir / "configs" / "layouts" / layout / "services"
    layout_services = {}
    if layout_services_dir.exists():
        for service_file in layout_services_dir.glob("*.yaml"):
            service_config = load_yaml(service_file)
            if "name" in service_config:
                layout_services[service_config["name"]] = service_config

    # Load overlay configs if provided
    overlay_common = {}
    overlay_services = {}
    if overlay:
        # Parse overlay path: "hybrid.testing.node-0" -> "hybrid/testing/node-0"
        overlay_path = overlay.replace(".", "/")
        overlay_dir = base_dir / "configs" / "overlays" / overlay_path

        # Load overlay common.yaml
        overlay_common_path = overlay_dir / "common.yaml"
        if overlay_common_path.exists():
            overlay_common = load_yaml(overlay_common_path)

        # Load overlay service configs
        overlay_services_dir = overlay_dir / "services"
        if overlay_services_dir.exists():
            for service_file in overlay_services_dir.glob("*.yaml"):
                service_config = load_yaml(service_file)
                if "name" in service_config:
                    overlay_services[service_config["name"]] = service_config

    # Merge configs in correct order to match deployment system:
    # 1. Layout service + Overlay service → merged service (overlay service overrides layout service)
    # 2. Layout common + Overlay common → merged common (overlay common overrides layout common)
    # 3. merged common merged into merged service → final config
    merged_services = []

    # First merge common configs: layout common + overlay common
    merged_common = deepcopy(layout_common)
    if overlay_common:
        merged_common = deep_merge_dict(merged_common, overlay_common)

    for service_name, layout_service in layout_services.items():
        # Start with layout service as base
        merged_service = deepcopy(layout_service)

        # Merge overlay service if it exists (overlay service overrides layout service)
        if service_name in overlay_services:
            merged_service = deep_merge_dict(merged_service, overlay_services[service_name])

        # Merge merged common into service (common can add/modify, service takes precedence)
        merged_service = deep_merge_dict(merged_service, merged_common)

        # Ensure name is set (service name always takes precedence)
        merged_service["name"] = service_name
        merged_services.append(merged_service)

    return merged_services
