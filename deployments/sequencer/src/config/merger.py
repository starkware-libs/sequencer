from copy import deepcopy
from pathlib import Path

import yaml
from src.config.loaders import DeploymentConfigLoader
from src.logging_config import get_logger
from src.config.overlay import (
    apply_services_overlay_strict,
    merge_common_with_overlay_strict,
)
from src.config.schema import CommonConfig
from src.config.schema import DeploymentConfig as DeploymentSchema
from src.config.schema import ServiceConfig

logger = get_logger(__name__)


def _flatten_dict_to_paths(d: dict, prefix: str = "") -> set[str]:
    """Flatten a dict to dot-separated key paths (leaf keys only)."""
    out: set[str] = set()
    for k, v in d.items():
        path = f"{prefix}.{k}" if prefix else k
        if isinstance(v, dict) and v:
            out |= _flatten_dict_to_paths(v, path)
        else:
            out.add(path)
    return out


def _load_common_yaml(path: str) -> dict | None:
    """Load and validate common.yaml; return dict or None if file missing/invalid."""
    p = Path(path)
    if not p.is_file():
        return None
    with open(p, "r", encoding="utf-8") as f:
        raw = yaml.safe_load(f) or {}
    validated = CommonConfig.model_validate(raw)
    return validated.model_dump(mode="python", exclude_unset=True, exclude_none=True)


def _merge_common_into_service(
    common_config: CommonConfig | dict | None, service_config: ServiceConfig
) -> ServiceConfig:
    """Merge common config into service config. Common first, service overrides. Special handling for service.ports (merge by name), config.sequencerConfig, config.configList."""
    service_dict = service_config.model_dump(mode="python", exclude_unset=True, exclude_none=True)
    if common_config is None:
        return service_config
    common_dict = (
        common_config
        if isinstance(common_config, dict)
        else common_config.model_dump(mode="python", exclude_unset=True, exclude_none=True)
    )
    if not common_dict:
        return service_config

    common_fields = set(CommonConfig.model_fields.keys()) & set(ServiceConfig.model_fields.keys())
    common_fields.discard("name")

    def deep_merge(base: dict, overlay: dict) -> dict:
        result = deepcopy(base)
        for key, value in overlay.items():
            if key in result and isinstance(result[key], dict) and isinstance(value, dict):
                result[key] = deep_merge(result[key], value)
            elif key not in result:
                result[key] = deepcopy(value)
        return result

    def merge_ports_by_name(common_ports: list, service_ports: list) -> list:
        if not common_ports:
            return service_ports
        if not service_ports:
            return common_ports
        common_by_name = {
            p["name"]: p for p in common_ports if isinstance(p, dict) and p.get("name")
        }
        merged = []
        seen = set()
        for p in service_ports:
            if isinstance(p, dict) and p.get("name"):
                seen.add(p["name"])
            merged.append(p)
        for name, p in common_by_name.items():
            if name not in seen:
                merged.append(p)
        return merged

    for field_name in common_fields:
        if field_name not in common_dict:
            continue
        common_val = common_dict[field_name]
        service_val = service_dict.get(field_name)

        if field_name == "service" and isinstance(common_val, dict) and common_val.get("ports"):
            if "service" not in service_dict:
                service_dict["service"] = {}
            svc_ports = service_dict["service"].get("ports", [])
            service_dict["service"]["ports"] = merge_ports_by_name(common_val["ports"], svc_ports)
            if service_val:
                rest = deep_merge(service_val, common_val)
                rest["ports"] = service_dict["service"]["ports"]
                service_dict["service"] = rest
            else:
                service_dict["service"] = {**common_val, "ports": service_dict["service"]["ports"]}
        elif field_name == "config" and isinstance(common_val, dict):
            merged_cfg = deepcopy(service_val) if service_val else {}
            if common_val.get("sequencerConfig"):
                if "sequencerConfig" not in merged_cfg:
                    merged_cfg["sequencerConfig"] = {}
                # Common last so env-level common (e.g. env_overlay) overrides per-node values
                merged_cfg["sequencerConfig"] = {
                    **merged_cfg.get("sequencerConfig", {}),
                    **deepcopy(common_val["sequencerConfig"]),
                }
            if "configList" in common_val:
                merged_cfg["configList"] = deepcopy(common_val["configList"])
            for k, v in common_val.items():
                if k in ("sequencerConfig", "configList"):
                    continue
                if k not in merged_cfg:
                    merged_cfg[k] = deepcopy(v)
                elif isinstance(merged_cfg.get(k), dict) and isinstance(v, dict):
                    merged_cfg[k] = deep_merge(merged_cfg[k], v)
            service_dict["config"] = merged_cfg
        elif service_val is None:
            service_dict[field_name] = common_val
        elif isinstance(common_val, dict) and isinstance(service_val, dict):
            service_dict[field_name] = deep_merge(service_val, common_val)
        else:
            service_dict[field_name] = service_val

    return ServiceConfig.model_validate(service_dict)


def merge_configs(
    layout_common_config_path: str | None,
    layout_services_config_dir_path: str,
    overlay_layers: list[tuple[str, str | None, str | None]] | None = None,
) -> DeploymentSchema:
    """
    Merge base (layout) configs with optional overlay layers.

    overlay_layers: list of (overlay_name, common_path, services_path). overlay_name is used
    for duplicate-key warning logs.

    Merge pipeline:
    1. Commons chain: layout_common <- overlay1_common <- overlay2_common -> merged_common
    2. Services chain: layout_services <- overlay1_services <- overlay2_services -> merged_services
    3. merged_common merged into each merged_service -> final_services

    Each overlay layer's common.yaml and services/ are optional; if absent that layer
    is skipped for that chain. Uses DeploymentConfigLoader for loading and validation.
    Logs a warning when a key is defined in more than one overlay (redundancy).
    Returns a validated DeploymentConfig schema object.
    """
    overlay_layers = overlay_layers or []
    # Resolve config_base_dir so includes like "configs/layouts/hybrid/common.yaml" work (relative to app root)
    layout_services_path = Path(layout_services_config_dir_path)
    config_base_dir = str(layout_services_path.parent.parent.parent.parent)

    # Track which overlay(s) define each key for duplicate warnings
    common_path_to_overlays: dict[str, list[str]] = {}
    service_path_to_overlays: dict[tuple[str, str], list[str]] = {}  # (service_name, path) -> overlays

    # --- Load layout configs ---
    layout_common = (
        _load_common_yaml(layout_common_config_path) if layout_common_config_path else None
    )
    layout_loader = DeploymentConfigLoader(
        configs_dir_path=layout_services_config_dir_path,
        config_base_dir=config_base_dir,
    )
    layout_services = layout_loader._load_service_configs_from_dir()

    merged_common = layout_common
    merged_services = layout_services

    # --- Apply each overlay layer in order (left-to-right, last wins) ---
    for overlay_name, overlay_common_path, overlay_services_path in overlay_layers:
        if overlay_services_path:
            overlay_loader = DeploymentConfigLoader(
                configs_dir_path=overlay_services_path,
                config_base_dir=config_base_dir,
            )
            overlay_services = overlay_loader._load_service_configs_from_dir()
            for svc in overlay_services:
                svc_dict = svc.model_dump(mode="python", exclude_unset=True, exclude_none=True)
                for key_path in _flatten_dict_to_paths(svc_dict):
                    key = (svc.name, key_path)
                    if key not in service_path_to_overlays:
                        service_path_to_overlays[key] = []
                    service_path_to_overlays[key].append(overlay_name)
            merged_services = apply_services_overlay_strict(merged_services, overlay_services)
        if overlay_common_path:
            overlay_common = _load_common_yaml(overlay_common_path)
            if overlay_common is not None:
                for key_path in _flatten_dict_to_paths(overlay_common):
                    if key_path not in common_path_to_overlays:
                        common_path_to_overlays[key_path] = []
                    common_path_to_overlays[key_path].append(overlay_name)
                merged_common = merge_common_with_overlay_strict(
                    merged_common, overlay_common, source=overlay_common_path
                )

    # --- Log duplicate-key warnings (non-redundant config: key in multiple overlays) ---
    def _format_overlays(overlay_names: list[str]) -> str:
        return ", ".join(f"[bold white]{o}[/]" for o in overlay_names)

    for key_path, overlays in common_path_to_overlays.items():
        if len(overlays) > 1:
            logger.warning(
                "Config key %r is defined in multiple overlays: %s. "
                "Prefer defining it in a single overlay (e.g. env) unless an override is intended.",
                key_path,
                _format_overlays(overlays),
            )
    for (svc_name, key_path), overlays in service_path_to_overlays.items():
        if len(overlays) > 1:
            logger.warning(
                "Service %r key %r is defined in multiple overlays: %s. "
                "Prefer defining it in a single overlay (e.g. env) unless an override is intended.",
                svc_name,
                key_path,
                _format_overlays(overlays),
            )

    # --- Merge common into each service (once at the end) ---
    final_services = [
        _merge_common_into_service(merged_common, service) for service in merged_services
    ]

    merged = {
        "common": merged_common if merged_common is not None else ServiceConfig(),
        "services": final_services,
    }
    return DeploymentSchema.model_validate(merged)
