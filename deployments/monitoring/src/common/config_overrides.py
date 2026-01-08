#!/usr/bin/env python3
"""
Generic configuration override mechanism for replacing placeholders in JSON objects.

This module provides functions to:
- Load YAML configuration files
- Replace placeholders in the format $$$_ITEM_NAME.FIELD_$$$ with values from config
- Validate that all placeholders have corresponding config entries
- Detect unused config keys

The placeholder format is: $$$_ITEM_NAME.FIELD_$$$ or $$$_ITEM_NAME.PART1.PART2_$$$
The config key format is: item_name.field or item_name.part1.part2 (all lowercase)
"""

import os
import re
from typing import Any, Callable, Optional

import yaml
from rich.console import Console

# Pattern to match $$$_ITEM_NAME.FIELD_$$$ or $$$_ITEM_NAME.PART1.PART2_$$$
# Allow digits in item name and field path (e.g., L1_MESSAGE_SCRAPER, EXPR)
PLACEHOLDER_PATTERN = r"\$\$\$_([A-Z0-9_]+)\.([A-Z0-9_.]+)_\$\$\$"


def load_config_file(config_path: Optional[str], logger_instance=None) -> dict:
    """
    Load YAML config file with overrides.
    Returns empty dict if file doesn't exist or path is None.

    Args:
        config_path: Path to the YAML config file
        logger_instance: Optional logger instance

    Returns:
        Dictionary with config overrides
    """
    if not config_path or not os.path.isfile(config_path):
        return {}

    log = logger_instance
    if log:
        log.debug(f"Loading config file: {config_path}")
    with open(config_path, "r") as f:
        config = yaml.safe_load(f) or {}
    if log:
        log.debug(f"Loaded config: {config}")
    return config


def extract_config_key_from_placeholder(placeholder: str) -> Optional[str]:
    """
    Extract the config key from a placeholder string.

    Args:
        placeholder: Placeholder string in format $$$_ITEM_NAME.FIELD_$$$

    Returns:
        Config key in format item_name.field (lowercase), or None if invalid format
    """
    match = re.match(PLACEHOLDER_PATTERN, placeholder)
    if match:
        item_name = match.group(1).lower()
        field_path = match.group(2).lower()
        return f"{item_name}.{field_path}"
    return None


def replace_placeholder_in_string(
    value: str,
    config: dict,
    logger_instance=None,
    item_name: Optional[str] = None,
    item_name_validator: Optional[Callable[[str, str], bool]] = None,
) -> str:
    """
    Replace placeholders in a string with values from config.
    Supports nested fields like LABELS.OG_PRIORITY.

    Args:
        value: The string that may contain placeholders
        config: The YAML config dictionary
        logger_instance: Optional logger instance
        item_name: Optional item name for validation (e.g., alert name, dashboard name)
        item_name_validator: Optional function(item_name, placeholder_item_name) -> bool
                           to validate if placeholder should be replaced

    Returns:
        The value with placeholders replaced, or original value if no match found
    """
    log = logger_instance

    def replace_match(match):
        placeholder_item_name = match.group(1).lower()
        placeholder_field_path = match.group(2).lower()
        full_placeholder = match.group(0)

        # Construct the config key: item_name.field or item_name.labels.og_priority
        config_key = f"{placeholder_item_name}.{placeholder_field_path}"

        # Optional validation: check if placeholder matches current item
        if item_name_validator:
            if not item_name_validator(item_name, placeholder_item_name):
                if log:
                    log.warning(
                        f"Placeholder {full_placeholder} doesn't match item '{item_name}'. "
                        f"Expected item name '{item_name.lower()}', got '{placeholder_item_name}'. "
                        f"Skipping replacement."
                    )
                return full_placeholder
        elif item_name:
            # Default validation: check if placeholder item name matches current item name
            if placeholder_item_name != item_name.lower():
                if log:
                    log.warning(
                        f"Placeholder {full_placeholder} doesn't match item '{item_name}'. "
                        f"Expected item name '{item_name.lower()}', got '{placeholder_item_name}'. "
                        f"Skipping replacement."
                    )
                return full_placeholder

        # Look up in config
        if config_key in config:
            replacement = str(config[config_key])
            if log:
                log.info(
                    f"Replacing {full_placeholder} with '{replacement}' "
                    f"(config key: '{config_key}')"
                )
            return replacement
        else:
            if log:
                log.warning(
                    f"No override found for placeholder {full_placeholder}. "
                    f"Expected config key: '{config_key}'. Keeping placeholder."
                )
            return full_placeholder

    # Replace all placeholders in the value
    result = re.sub(PLACEHOLDER_PATTERN, replace_match, value)
    return result


def collect_placeholders_recursive(
    obj: Any,
    path: str = "",
    item_name_filter: Optional[str] = None,
) -> set[tuple[str, str, str]]:
    """
    Recursively collect all placeholders from an object.

    Args:
        obj: The object to process (dict, list, or primitive)
        path: Current path in the object (for error messages)
        item_name_filter: Optional item name to filter placeholders (e.g., alert name)
                        If provided, only placeholders matching this item name are collected

    Returns:
        Set of tuples: (full_placeholder, config_key, field_path)
    """
    placeholders = set()

    if isinstance(obj, dict):
        for key, value in obj.items():
            current_path = f"{path}.{key}" if path else key
            placeholders.update(
                collect_placeholders_recursive(value, current_path, item_name_filter)
            )
    elif isinstance(obj, list):
        for i, item in enumerate(obj):
            current_path = f"{path}[{i}]" if path else f"[{i}]"
            placeholders.update(
                collect_placeholders_recursive(item, current_path, item_name_filter)
            )
    elif isinstance(obj, str):
        # Find all placeholders in this string
        for match in re.finditer(PLACEHOLDER_PATTERN, obj):
            placeholder_item_name = match.group(1).lower()
            placeholder_field_path = match.group(2).lower()
            full_placeholder = match.group(0)

            # Optional filtering by item name
            if item_name_filter and placeholder_item_name != item_name_filter.lower():
                continue

            config_key = f"{placeholder_item_name}.{placeholder_field_path}"
            placeholders.add((full_placeholder, config_key, path))

    return placeholders


def replace_placeholders_recursive(
    obj: Any,
    config: dict,
    logger_instance=None,
    path: str = "",
    item_name: Optional[str] = None,
    item_name_validator: Optional[Callable[[str, str], bool]] = None,
) -> Any:
    """
    Recursively search through an object and replace placeholders in any string field.

    Args:
        obj: The object to process (dict, list, or primitive)
        config: The YAML config dictionary
        logger_instance: Optional logger instance
        path: Current path in the object (for logging)
        item_name: Optional item name for validation
        item_name_validator: Optional function to validate item name matching

    Returns:
        The object with placeholders replaced
    """
    log = logger_instance

    if isinstance(obj, dict):
        # Recursively process dictionary
        result = {}
        for key, value in obj.items():
            current_path = f"{path}.{key}" if path else key
            result[key] = replace_placeholders_recursive(
                value, config, logger_instance, current_path, item_name, item_name_validator
            )
        return result
    elif isinstance(obj, list):
        # Recursively process list
        result = []
        for i, item in enumerate(obj):
            current_path = f"{path}[{i}]" if path else f"[{i}]"
            result.append(
                replace_placeholders_recursive(
                    item, config, logger_instance, current_path, item_name, item_name_validator
                )
            )
        return result
    elif isinstance(obj, str):
        # Check if string contains a placeholder and replace it
        original_value = obj
        replaced_value = replace_placeholder_in_string(
            obj, config, logger_instance, item_name, item_name_validator
        )
        if replaced_value != original_value:
            if log:
                item_context = f"item '{item_name}'" if item_name else "item"
                log.info(
                    f"Applied override for {item_context}, field '{path}': "
                    f"'{original_value}' -> '{replaced_value}'"
                )
            return replaced_value
        return obj
    else:
        # Return primitive types as-is (int, float, bool, None)
        return obj


def validate_config_overrides(
    items: list[dict[str, Any]],
    config: dict,
    source_json_path: str = "",
    config_override_path: str = "",
    logger_instance=None,
    item_name_extractor: Optional[Callable[[dict], str]] = None,
    item_title_extractor: Optional[Callable[[dict], str]] = None,
    item_type_name: str = "item",
) -> None:
    """
    Validate that all placeholders in items have corresponding config entries,
    and that all config keys have corresponding placeholders.

    Args:
        items: List of item dictionaries (e.g., alerts, dashboards)
        config: The YAML config dictionary
        source_json_path: Path to the source JSON file (for error messages)
        config_override_path: Path to the config YAML file (for error messages)
        logger_instance: Optional logger instance
        item_name_extractor: Optional function(item) -> str to extract item name
                           Default: item["name"]
        item_title_extractor: Optional function(item) -> str to extract item title/description
                            Default: item.get("title", "N/A")
        item_type_name: Name of the item type for error messages (e.g., "alert", "dashboard")

    Raises:
        ValueError: If any placeholder is missing from config or any config key is unused
    """

    # Default extractors
    if item_name_extractor is None:
        item_name_extractor = lambda item: item["name"]
    if item_title_extractor is None:
        item_title_extractor = lambda item: item.get("title", "N/A")

    # Collect all placeholders from all items
    all_placeholders = set()  # Set of all config keys that have placeholders
    all_missing_placeholders = (
        []
    )  # List of (item_name, item_title, placeholder, config_key, field_path)

    for item in items:
        item_name = item_name_extractor(item)
        item_title = item_title_extractor(item)

        # Collect all placeholders for this item
        placeholders = collect_placeholders_recursive(item, item_name_filter=item_name)

        # Check which placeholders are missing from config
        for full_placeholder, config_key, field_path in placeholders:
            all_placeholders.add(config_key)  # Track all placeholder config keys
            if config_key not in config:
                all_missing_placeholders.append(
                    (item_name, item_title, full_placeholder, config_key, field_path)
                )

    # Check for unused config keys (keys in config that don't have corresponding placeholders)
    unused_config_keys = list(set(config.keys()) - all_placeholders)

    # If there are missing placeholders OR unused config keys, show error
    if not all_missing_placeholders and not unused_config_keys:
        # No issues, validation passed
        return

    # Build comprehensive error message using Rich
    console = Console()
    total_issues = len(all_missing_placeholders) + len(unused_config_keys)
    error_title = "CONFIGURATION ERRORS DETECTED"
    if all_missing_placeholders and unused_config_keys:
        error_title = "CONFIGURATION ERRORS DETECTED (Missing Placeholders & Unused Config Keys)"
    elif all_missing_placeholders:
        error_title = "UNHANDLED PLACEHOLDER(S) DETECTED"
    elif unused_config_keys:
        error_title = "UNUSED CONFIG KEY(S) DETECTED"

    # Build error message with Rich markup
    error_parts = [
        "[bold red]" + "=" * 80 + "[/bold red]",
        f"[bold red]ERROR:[/bold red] [bold]{error_title}[/bold]",
        "[bold red]" + "=" * 80 + "[/bold red]",
        "",
        f"Found [yellow]{total_issues}[/yellow] issue(s):",
        f"  - [cyan]{len(all_missing_placeholders)}[/cyan] unhandled placeholder(s) across [cyan]{len(set(p[0] for p in all_missing_placeholders)) if all_missing_placeholders else 0}[/cyan] {item_type_name}(s)",
        f"  - [cyan]{len(unused_config_keys)}[/cyan] unused config key(s)",
        "",
        "[bold]File Paths:[/bold]",
    ]

    if source_json_path:
        error_parts.append(f"  source_json_path: [cyan]{source_json_path}[/cyan]")
    else:
        error_parts.append("  source_json_path: [dim]<not provided>[/dim]")

    if config_override_path:
        error_parts.append(f"  config_override_path: [cyan]{config_override_path}[/cyan]")
    else:
        error_parts.append("  config_override_path: [dim]<not provided>[/dim]")

    error_parts.append("")

    # Display missing placeholders in simple list format
    if all_missing_placeholders:
        error_parts.append("[bold]" + "-" * 80 + "[/bold]")
        error_parts.append("[bold]Missing Placeholders (in JSON but not in YAML):[/bold]")
        error_parts.append("[bold]" + "-" * 80 + "[/bold]")

        # Simple list: placeholder on one line, config key on next line
        for item_name, item_title, placeholder, config_key, field_path in all_missing_placeholders:
            error_parts.append(f"[red]{placeholder}[/red]")
            error_parts.append(f"[green]{config_key}[/green]")
            error_parts.append("")  # Empty line between pairs

        # Remove trailing empty line
        error_parts.pop()

    # Display unused config keys in simple list format
    if unused_config_keys:
        error_parts.append("[bold]" + "-" * 80 + "[/bold]")
        error_parts.append("[bold]Unused Config Keys (in YAML but not in JSON):[/bold]")
        error_parts.append("[bold]" + "-" * 80 + "[/bold]")

        # Convert config keys to placeholder format for display
        for config_key in sorted(unused_config_keys):
            # Convert config key back to placeholder format
            # item.field -> $$$_ITEM_FIELD_$$$
            placeholder = f"$$$_{config_key.upper().replace('.', '_')}_$$$"
            error_parts.append(f"[yellow]{config_key}[/yellow]")
            error_parts.append(f"[dim]{placeholder}[/dim]")
            error_parts.append("")  # Empty line between pairs

        # Remove trailing empty line
        error_parts.pop()

    # Single unified "To fix" section at the bottom
    error_parts.append("")
    error_parts.append("[bold]To fix:[/bold]")
    if all_missing_placeholders and unused_config_keys:
        error_parts.append(
            "  For missing placeholders: Add the corresponding config keys to your YAML config file,\n"
            "  or remove the placeholders from the source JSON file."
        )
        error_parts.append(
            "  For unused config keys: Remove the unused keys from your YAML config file, or add\n"
            "  corresponding placeholders to the source JSON file."
        )
    elif all_missing_placeholders:
        error_parts.append(
            "  Add the corresponding config keys to your YAML config file, or remove the\n"
            "  placeholders from the source JSON file."
        )
    elif unused_config_keys:
        error_parts.append(
            "  Remove the unused config keys from your YAML config file, or add corresponding\n"
            "  placeholders to the source JSON file."
        )

    error_parts.append("")
    error_parts.append("[bold red]" + "=" * 80 + "[/bold red]")

    # Build the error message string with Rich formatting
    rich_error_message = "\n".join(error_parts)

    # Print with Rich formatting
    console.print(rich_error_message)

    # Build plain text version for ValueError (strip Rich markup)
    plain_error_parts = []
    for part in error_parts:
        # Remove Rich markup tags like [bold], [red], etc.
        plain_part = re.sub(r"\[/?[^\]]+\]", "", part)
        plain_error_parts.append(plain_part)

    plain_error_message = "\n".join(plain_error_parts)

    # Don't log the error here - it's already printed with Rich formatting above
    # The ValueError will be caught by the caller and handled appropriately
    raise ValueError(plain_error_message)


def apply_config_overrides(
    item: dict[str, Any],
    config: dict,
    logger_instance=None,
    item_name: Optional[str] = None,
    item_name_validator: Optional[Callable[[str, str], bool]] = None,
    post_process: Optional[Callable[[dict], dict]] = None,
) -> dict[str, Any]:
    """
    Apply config overrides to item fields that contain placeholders.
    Recursively searches through the entire item object to find and replace placeholders in ANY field.

    Args:
        item: The item dictionary (e.g., alert, dashboard)
        config: The YAML config dictionary
        logger_instance: Optional logger instance
        item_name: Optional item name for validation
        item_name_validator: Optional function to validate item name matching
        post_process: Optional function(item) -> item to apply post-processing
                    (e.g., type conversions, field transformations)

    Returns:
        A copy of item with placeholders replaced
    """

    # Recursively process the entire item object
    item_copy = replace_placeholders_recursive(
        item, config, logger_instance, item_name=item_name, item_name_validator=item_name_validator
    )

    # Apply post-processing if provided
    if post_process:
        item_copy = post_process(item_copy)

    return item_copy
