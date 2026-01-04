import json
import os
from abc import ABC, abstractmethod
from pathlib import Path
from typing import Any, Dict, List, Optional

import yaml
from src.config.schema import CommonConfig, ServiceConfig


class Config(ABC):
    """Base configuration class with validation and safe load utilities."""

    @abstractmethod
    def load(self) -> dict:
        pass

    @abstractmethod
    def _validate(self):
        pass

    def _validate_file(self, file_path: str) -> None:
        """Ensure file exists, is readable, and is a real file."""
        path = Path(file_path)
        if not path.exists():
            raise ValueError(f"File path {file_path} does not exist")
        if not path.is_file():
            raise ValueError(f"File path {file_path} is not a file")
        if not os.access(path, os.R_OK):
            raise ValueError(f"File path {file_path} is not readable")

    def _validate_directory(self, dir_path: str) -> None:
        """Ensure directory exists and is readable."""
        path = Path(dir_path)
        if not path.exists():
            raise ValueError(f"Directory path {dir_path} does not exist")
        if not path.is_dir():
            raise ValueError(f"Directory path {dir_path} is not a directory")
        if not os.access(dir_path, os.R_OK):
            raise ValueError(f"Directory path {dir_path} is not readable")

    def _try_load_yaml(self, file_path: str) -> dict:
        """Validate and load YAML safely."""
        self._validate_file(file_path)
        if not file_path.endswith((".yaml", ".yml")):
            raise ValueError(f"File path {file_path} is not a YAML file")
        try:
            with open(file_path, "r", encoding="utf-8") as f:
                return yaml.safe_load(f) or {}
        except yaml.YAMLError as e:
            raise ValueError(f"Invalid YAML in {file_path}: {e}")

    def _try_load_json(self, file_path: str) -> dict:
        """Validate and load JSON safely."""
        self._validate_file(file_path)
        if not file_path.endswith(".json"):
            raise ValueError(f"File path {file_path} is not a JSON file")
        try:
            with open(file_path, "r", encoding="utf-8") as f:
                return json.load(f)
        except json.JSONDecodeError as e:
            raise ValueError(f"Invalid JSON in {file_path}: {e}")


class DeploymentConfigLoader(Config):
    """Loads and validates service and common YAML configs."""

    def __init__(self, configs_dir_path: str, common_config_path: Optional[str] = None):
        self.configs_dir_path = Path(configs_dir_path)
        self.common_config_path = Path(common_config_path) if common_config_path else None
        self._validate()

    def _validate(self):
        """Validate directory existence and file readability."""
        self._validate_directory(self.configs_dir_path)
        # Note: common_config_path is optional - it may not exist in overlay paths

    def _load_service_configs_from_dir(self) -> List[ServiceConfig]:
        """Load and validate each service YAML file in the directory."""
        validated_configs = []
        for fname in os.listdir(self.configs_dir_path):
            if not fname.endswith((".yaml", ".yml")):
                continue
            file_path = self.configs_dir_path / fname
            raw_config = self._try_load_yaml(str(file_path))
            validated_config = ServiceConfig.model_validate(raw_config)
            # Validate that service configs have a name (required for services, optional for common)
            if not validated_config.name:
                raise ValueError(
                    f"Service config file '{file_path}' is missing required field 'name'. "
                    f"Service configs must have a name field."
                )
            validated_config._source = str(file_path)
            validated_configs.append(validated_config)
        return validated_configs

    def _wrap_services(self, services: list) -> dict:
        """Wrap the list of services in a 'services' key."""
        return {"services": services}

    def _load_common_config(self) -> Optional[dict]:
        """Optionally load and validate a common config file."""
        if not self.common_config_path:
            return None
        # Check if file exists - common.yaml is optional
        if not self.common_config_path.exists():
            return None
        raw = self._try_load_yaml(str(self.common_config_path))
        validated_model = CommonConfig.model_validate(raw)
        # Use exclude_unset=True to avoid including fields with default_factory that weren't explicitly set
        validated = validated_model.model_dump(mode="python", exclude_unset=True, exclude_none=True)
        return validated

    def load(self) -> dict:
        """Load all service configs and optionally merge with common config."""
        services = self._load_service_configs_from_dir()
        wrapped = self._wrap_services(services)
        common = self._load_common_config()
        return {**common, **wrapped} if common else wrapped


class NodeConfigLoader(Config):
    ROOT_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../../../")

    def __init__(self, config_list_json_path: str):
        """
        Initialize NodeConfigLoader.

        Args:
            config_list_json_path: Path to JSON file containing a list of config paths
        """
        self.config_list_json_path = config_list_json_path
        self._validate()

    def _validate(self):
        # Validate the config list JSON file
        full_path = os.path.join(self.ROOT_DIR, self.config_list_json_path)
        self._validate_file(full_path)

    def load(self) -> dict:
        # Load the JSON file containing the list of config paths
        config_list_full_path = os.path.join(self.ROOT_DIR, self.config_list_json_path)
        config_data = self._try_load_json(file_path=config_list_full_path)

        # Validate that it's a list of strings
        if not isinstance(config_data, list):
            raise ValueError(
                f"Config list JSON file '{self.config_list_json_path}' must contain a JSON array. Got: {type(config_data)}"
            )
        config_list: List[str] = config_data
        if not all(isinstance(item, str) for item in config_list):
            raise ValueError(
                f"Config list JSON file '{self.config_list_json_path}' must contain a JSON array of strings."
            )

        # Load and merge all config files in the list
        result = {}
        for config_path in config_list:
            # Use the full path as provided in the config list
            config_full_path = os.path.join(self.ROOT_DIR, config_path)
            try:
                data = self._try_load_json(file_path=config_full_path)
                if not isinstance(data, dict):
                    raise ValueError(
                        f"Config file '{config_path}' must contain a JSON object (dict), got: {type(data)}"
                    )
                result.update(data)  # later values overwrite previous
            except FileNotFoundError:
                # Fail fast if a specified config file doesn't exist
                raise FileNotFoundError(f"Config file not found: {config_full_path}")
            except ValueError as e:
                # Fail fast if a config file has invalid JSON
                raise ValueError(f"Invalid JSON in config file {config_full_path}: {e}")

        # Return a lexicographically sorted dict to ensure consistent ordering and simpler CM diffs.
        return dict[Any, Any](sorted(result.items()))

    @staticmethod
    def _set_nested_dotted_key(data: dict, dotted_key: str, value: Any) -> None:
        """Set value in nested dict using dotted key notation, creating structure if needed.

        Examples:
            _set_nested_dotted_key({}, 'a.b.c', 123) -> {'a': {'b': {'c': 123}}}
            _set_nested_dotted_key({'a': {'x': 1}}, 'a.b.c', 123) -> {'a': {'x': 1, 'b': {'c': 123}}}
        """
        keys = dotted_key.split(".")
        current = data
        for key in keys[:-1]:
            if key not in current or not isinstance(current[key], dict):
                current[key] = {}
            current = current[key]
        current[keys[-1]] = value

    @staticmethod
    def _replace_placeholder_value(obj: Any, placeholder: str, replacement: Any) -> Any:
        """Recursively search for a placeholder value and replace it with the replacement value.

        Args:
            obj: The object to search (dict, list, or primitive)
            placeholder: The placeholder string to find (e.g., '$$$_CHAIN_ID_$$$')
            replacement: The value to replace it with

        Returns:
            The object with placeholder replaced (if found)
        """
        if isinstance(obj, dict):
            return {
                k: NodeConfigLoader._replace_placeholder_value(v, placeholder, replacement)
                for k, v in obj.items()
            }
        elif isinstance(obj, list):
            return [
                NodeConfigLoader._replace_placeholder_value(item, placeholder, replacement)
                for item in obj
            ]
        elif isinstance(obj, str) and obj == placeholder:
            return replacement
        elif isinstance(obj, (int, float)) and str(obj) == placeholder:
            return replacement
        else:
            return obj

    @staticmethod
    def _placeholder_exists(obj: Any, placeholder: str) -> bool:
        """Check if a placeholder value exists anywhere in the config object.

        Args:
            obj: The object to search (dict, list, or primitive)
            placeholder: The placeholder string to find (e.g., '$$$_CHAIN_ID_$$$')

        Returns:
            True if placeholder is found, False otherwise
        """
        if isinstance(obj, dict):
            return any(NodeConfigLoader._placeholder_exists(v, placeholder) for v in obj.values())
        elif isinstance(obj, list):
            return any(NodeConfigLoader._placeholder_exists(item, placeholder) for item in obj)
        elif isinstance(obj, str) and obj == placeholder:
            return True
        elif isinstance(obj, (int, float)) and str(obj) == placeholder:
            return True
        else:
            return False

    @staticmethod
    def _normalize_placeholder(placeholder: str) -> str:
        """Normalize placeholder by replacing dashes with underscores.

        Only affects values matching the $$$_..._$$$ pattern. This ensures all
        placeholders use underscores consistently for matching.

        Args:
            placeholder: The placeholder string (e.g., '$$$_COMPONENTS-SIERRA-COMPILER-URL_$$$')

        Returns:
            Normalized placeholder with dashes replaced by underscores
            (e.g., '$$$_COMPONENTS_SIERRA_COMPILER_URL_$$$')

        Examples:
            '$$$_COMPONENTS-SIERRA-COMPILER-URL_$$$' -> '$$$_COMPONENTS_SIERRA_COMPILER_URL_$$$'
            '$$$_CHAIN_ID_$$$' -> '$$$_CHAIN_ID_$$$' (no change)
            'regular_string' -> 'regular_string' (no change)
        """
        if (
            isinstance(placeholder, str)
            and placeholder.startswith("$$$_")
            and placeholder.endswith("_$$$")
        ):
            # Replace dashes with underscores in the middle part (between $$$_ and _$$$)
            middle = placeholder[4:-4]  # Remove $$$_ prefix and _$$$ suffix
            normalized_middle = middle.replace("-", "_")
            return f"$$$_{normalized_middle}_$$$"
        return placeholder

    @staticmethod
    def _normalize_placeholders_in_config(obj: Any) -> Any:
        """Recursively normalize all placeholders in a config object.

        Traverses dictionaries, lists, and primitive values, replacing dashes
        with underscores in all placeholder values matching $$$_..._$$$ pattern.

        Args:
            obj: The config object to normalize (dict, list, or primitive)

        Returns:
            The normalized config object with all placeholders normalized
        """
        if isinstance(obj, dict):
            return {
                k: NodeConfigLoader._normalize_placeholders_in_config(v) for k, v in obj.items()
            }
        elif isinstance(obj, list):
            return [NodeConfigLoader._normalize_placeholders_in_config(item) for item in obj]
        elif isinstance(obj, str):
            return NodeConfigLoader._normalize_placeholder(obj)
        else:
            # For int, float, bool, etc., check if string representation is a placeholder
            if isinstance(obj, (int, float)):
                str_repr = str(obj)
                if str_repr.startswith("$$$_") and str_repr.endswith("_$$$"):
                    normalized = NodeConfigLoader._normalize_placeholder(str_repr)
                    # Try to preserve original type if possible
                    try:
                        if isinstance(obj, int):
                            return int(normalized)
                        elif isinstance(obj, float):
                            return float(normalized)
                    except (ValueError, TypeError):
                        return normalized
            return obj

    @staticmethod
    def _yaml_key_to_placeholder(yaml_key: str) -> str:
        """Convert a YAML key to the full placeholder format.

        YAML keys use hierarchical structure with dots (e.g., 'components.batcher.port'),
        which map to placeholders with hyphens (e.g., '$$$_COMPONENTS-BATCHER-PORT_$$$').

        The transformation:
        - Dots (.) in YAML → Hyphens (-) in placeholder
        - Lowercase → Uppercase
        - Special keys with '.#is_none' → '.#is_none' becomes '-IS_NONE' (hash removed)

        Args:
            yaml_key: The YAML key with dots (e.g., 'components.batcher.port', 'chain_id')

        Returns:
            The full placeholder format (e.g., '$$$_COMPONENTS-BATCHER-PORT_$$$', '$$$_CHAIN_ID_$$$')

        Examples:
            'components.batcher.port' -> '$$$_COMPONENTS-BATCHER-PORT_$$$'
            'components.sierra_compiler.url' -> '$$$_COMPONENTS-SIERRA_COMPILER-URL_$$$'
            'consensus_manager_config.context_config.override_eth_to_fri_rate.#is_none' -> '$$$_CONSENSUS_MANAGER_CONFIG-CONTEXT_CONFIG-OVERRIDE_ETH_TO_FRI_RATE-IS_NONE_$$$'
            'chain_id' -> '$$$_CHAIN_ID_$$$' (single-level keys still work)
        """
        # Convert dots to hyphens, then uppercase
        # Special handling: '.#is_none' becomes '-IS_NONE' (remove the #)
        placeholder = yaml_key.replace(".#is_none", "-is_none").replace(".", "-").upper()
        return f"$$$_{placeholder}_$$$"

    @staticmethod
    def _placeholder_to_yaml_key(placeholder: str) -> str:
        """Convert a placeholder back to YAML key format.

        This is the inverse of _yaml_key_to_placeholder.

        Args:
            placeholder: The placeholder string (e.g., '$$$_COMPONENTS-BATCHER-PORT_$$$')

        Returns:
            The YAML key format (e.g., 'components.batcher.port')

        Examples:
            '$$$_COMPONENTS-BATCHER-PORT_$$$' -> 'components.batcher.port'
            '$$$_CONSENSUS_MANAGER_CONFIG-CONTEXT_CONFIG-OVERRIDE_ETH_TO_FRI_RATE-IS_NONE_$$$' -> 'consensus_manager_config.context_config.override_eth_to_fri_rate.#is_none'
        """
        # Remove $$$_ prefix and _$$$ suffix
        middle = placeholder[4:-4]
        # Convert hyphens to dots and lowercase
        yaml_key = middle.replace("-", ".").lower()
        # Special handling: convert '-is_none' back to '.#is_none'
        yaml_key = yaml_key.replace(".is_none", ".#is_none")
        return yaml_key

    @staticmethod
    def apply_sequencer_overrides(
        merged_json_config: dict,
        sequencer_config: Dict[str, Any],
        service_name: str = "unknown",
        config_list_path: Optional[str] = None,
        overlay_source: Optional[str] = None,
    ) -> dict:
        """Apply sequencerConfig overrides from YAML to merged JSON config.

        Overrides are applied by placeholder value, not by JSON key. This makes the
        deployment resilient to JSON key changes as long as the placeholder values remain the same.

        YAML keys use hierarchical structure with dots (e.g., 'components.batcher.port'),
        which are automatically converted to placeholder format (e.g., '$$$_COMPONENTS-BATCHER-PORT_$$$')
        for matching against placeholders in the JSON config.

        Args:
            merged_json_config: The merged JSON config dictionary from all config files
            sequencer_config: Dictionary from YAML with hierarchical keys:
                {
                    'components.batcher.port': 55000,
                    'components.sierra_compiler.url': 'sequencer-sierracompiler-service',
                    'chain_id': '123'
                }
                These are converted to placeholder format for matching.
            service_name: Name of the service (for error messages)
            config_list_path: Optional path to the config list JSON file (for error messages)
            overlay_source: Optional source identifier for the overlay file (for error messages)

        Returns:
            Updated config dictionary with overrides applied

        Raises:
            ValueError: If any YAML key doesn't match any placeholder in the config

        Examples:
            JSON: {'components.batcher.port': '$$$_COMPONENTS-BATCHER-PORT_$$$'}
            YAML: {'components.batcher.port': 55000}
            Result: {'components.batcher.port': 55000}
        """
        # Use config as-is (no normalization needed - placeholders already use hyphens)
        result = merged_json_config

        # Step 1: Validate that all YAML keys match existing placeholders
        # Collect unmatched keys but don't raise yet - we'll check after applying overrides
        unmatched_keys = []
        matched_keys = []
        for yaml_key, replacement_value in sequencer_config.items():
            # Convert YAML key to placeholder format (dots -> hyphens, uppercase)
            placeholder = NodeConfigLoader._yaml_key_to_placeholder(yaml_key)
            # Check if placeholder exists in the config
            exists = NodeConfigLoader._placeholder_exists(result, placeholder)
            if not exists:
                unmatched_keys.append((yaml_key, placeholder))
            else:
                matched_keys.append((yaml_key, placeholder, replacement_value))

        # Step 2: Apply overrides for matched keys only
        for yaml_key, placeholder, replacement_value in matched_keys:
            # Replace the placeholder value wherever it appears in the config
            result = NodeConfigLoader._replace_placeholder_value(
                result, placeholder, replacement_value
            )

        # Step 3: Check for remaining placeholders
        remaining_placeholders = NodeConfigLoader._find_all_placeholders(result)

        # Step 4: If there are any issues, raise a combined error
        if unmatched_keys or remaining_placeholders:
            total_issues = len(unmatched_keys) + len(remaining_placeholders)
            error_message = "=" * 80 + "\n"
            error_message += "ERROR: CONFIGURATION ERRORS DETECTED (Unused Config Keys & Unhandled Placeholders)\n"
            error_message += "=" * 80 + "\n\n"
            error_message += f"Found {total_issues} issue(s):\n"
            if unmatched_keys:
                error_message += f"  - {len(unmatched_keys)} unused config key(s)\n"
            if remaining_placeholders:
                error_message += f"  - {len(remaining_placeholders)} unhandled placeholder(s)\n"
            error_message += "\n"

            # File paths section
            error_message += "File Paths:\n"
            if config_list_path:
                full_config_path = os.path.join(NodeConfigLoader.ROOT_DIR, config_list_path)
                error_message += f"  application_config_json_path: {full_config_path}\n"
            else:
                error_message += "  application_config_json_path: <unknown>\n"
            if overlay_source:
                error_message += f"  config_override_path: {overlay_source}\n"
            else:
                error_message += "  config_override_path: <unknown>\n"

            # Unused Config Keys section
            if unmatched_keys:
                error_message += "\n" + "-" * 80 + "\n"
                error_message += "Unused Config Keys:\n"
                error_message += "-" * 80 + "\n"
                error_message += (
                    f"  Found {len(unmatched_keys)} config key(s) in your YAML file that don't have\n"
                    f"  corresponding placeholders in the application config JSON file:\n\n"
                )

                for idx, (yaml_key, placeholder) in enumerate(unmatched_keys, 1):
                    error_message += f"  Unused Config Key #{idx}:\n"
                    error_message += f"    YAML key: {yaml_key}\n"
                    error_message += f"    Maps to placeholder: {placeholder}\n"
                    error_message += (
                        f"\n    This key exists in your config file but no placeholder matches it.\n"
                        f"    Either remove this key from your config file, or add a corresponding\n"
                        f"    placeholder to the application config JSON file.\n"
                    )
                    if idx < len(unmatched_keys):
                        error_message += "\n"

                error_message += "\n"
                error_message += "  To fix:\n"
                error_message += (
                    "    Remove the unused config keys from your YAML overlay file, or add\n"
                    "    corresponding placeholders to the application config JSON file.\n"
                )

            # Unhandled Placeholders section
            if remaining_placeholders:
                sorted_placeholders = sorted(remaining_placeholders)
                error_message += "\n" + "-" * 80 + "\n"
                error_message += "Missing Placeholders:\n"
                error_message += "-" * 80 + "\n"
                error_message += (
                    f"  The following {len(sorted_placeholders)} placeholder(s) were found in the\n"
                    f"  application config but were not overridden in your YAML overlay:\n\n"
                )

                for idx, placeholder in enumerate(sorted_placeholders, 1):
                    # Find where this placeholder appears in the config
                    locations = NodeConfigLoader._find_placeholder_locations(result, placeholder)
                    # Convert placeholder back to YAML key format for suggestion
                    yaml_key_suggestion = NodeConfigLoader._placeholder_to_yaml_key(placeholder)

                    error_message += f"  Missing Placeholder #{idx}:\n\n"
                    error_message += f"    Placeholder:\n"
                    error_message += f"      {placeholder}\n\n"
                    if locations:
                        error_message += f"    Location(s) in JSON:\n"
                        for loc in locations[:3]:  # Show up to 3 locations
                            error_message += f"      key path: {loc}\n"
                        if len(locations) > 3:
                            error_message += f"      ... and {len(locations) - 3} more location(s)\n"
                    error_message += f"\n    Expected config key:\n"
                    error_message += f"      {yaml_key_suggestion}\n\n"
                    error_message += f"    Add to your config file as:\n"
                    error_message += f"      {yaml_key_suggestion}: <value>\n"
                    if idx < len(sorted_placeholders):
                        error_message += "\n"

                error_message += "\n"
                error_message += "  To fix:\n"
                error_message += (
                    "    Add the missing placeholder entries to your YAML overlay file, or\n"
                    "    remove the placeholders from the application config JSON file.\n"
                )

            error_message += "\n" + "=" * 80

            raise ValueError(error_message)

        # Re-sort after modifications
        return dict[Any, Any](sorted(result.items()))

    @staticmethod
    def _find_all_placeholders(obj: Any) -> set[str]:
        """Recursively find all placeholder values ($$$_..._$$$) in the config object.

        Args:
            obj: The object to search (dict, list, or primitive)

        Returns:
            Set of all placeholder values found (e.g., {'$$$_CHAIN_ID_$$$', '$$$_STARKNET_URL_$$$'})
        """
        placeholders: set[str] = set()

        if isinstance(obj, dict):
            for value in obj.values():
                placeholders.update(NodeConfigLoader._find_all_placeholders(value))
        elif isinstance(obj, list):
            for item in obj:
                placeholders.update(NodeConfigLoader._find_all_placeholders(item))
        elif isinstance(obj, str) and obj.startswith("$$$_") and obj.endswith("_$$$"):
            placeholders.add(obj)
        elif isinstance(obj, (int, float)):
            str_repr = str(obj)
            if str_repr.startswith("$$$_") and str_repr.endswith("_$$$"):
                placeholders.add(str_repr)

        return placeholders

    @staticmethod
    def _find_placeholder_locations(obj: Any, placeholder: str, path: str = "") -> List[str]:
        """Recursively find all key paths where a placeholder value appears in the config object.

        Args:
            obj: The object to search (dict, list, or primitive)
            placeholder: The placeholder string to find (e.g., '$$$_CHAIN_ID_$$$')
            path: Current path in the object (for building full paths)

        Returns:
            List of key paths where the placeholder was found (e.g., ['chain_id', 'components.batcher.port'])
        """
        locations: List[str] = []

        if isinstance(obj, dict):
            for key, value in obj.items():
                current_path = f"{path}.{key}" if path else key
                locations.extend(
                    NodeConfigLoader._find_placeholder_locations(value, placeholder, current_path)
                )
        elif isinstance(obj, list):
            for idx, item in enumerate(obj):
                current_path = f"{path}[{idx}]" if path else f"[{idx}]"
                locations.extend(
                    NodeConfigLoader._find_placeholder_locations(item, placeholder, current_path)
                )
        elif isinstance(obj, str) and obj == placeholder:
            locations.append(path if path else "<root>")
        elif isinstance(obj, (int, float)) and str(obj) == placeholder:
            locations.append(path if path else "<root>")

        return locations

    @staticmethod
    def validate_no_remaining_placeholders(
        config: dict,
        config_list_path: Optional[str] = None,
        overlay_source: Optional[str] = None,
    ) -> None:
        """Validate that no placeholder values remain in the final config.

        Args:
            config: The final config dictionary after all overrides are applied
            config_list_path: Optional path to the config list JSON file (for error messages)
            overlay_source: Optional source identifier for the overlay file (for error messages)

        Raises:
            ValueError: If any placeholder values ($$$_..._$$$) are found in the config
        """
        remaining_placeholders = NodeConfigLoader._find_all_placeholders(config)

        if remaining_placeholders:
            sorted_placeholders = sorted(remaining_placeholders)

            error_message = "=" * 80 + "\n"
            error_message += "ERROR: UNHANDLED PLACEHOLDERS DETECTED\n"
            error_message += "=" * 80 + "\n\n"
            error_message += (
                f"Found {len(sorted_placeholders)} unhandled placeholder(s) in the final config.\n\n"
            )

            # File paths section
            error_message += "File Paths:\n"
            if config_list_path:
                full_config_path = os.path.join(NodeConfigLoader.ROOT_DIR, config_list_path)
                error_message += f"  application_config_json_path: {full_config_path}\n"
            else:
                error_message += "  application_config_json_path: <unknown>\n"
            if overlay_source:
                error_message += f"  config_override_path: {overlay_source}\n"
            else:
                error_message += "  config_override_path: <unknown>\n"

            error_message += "\n" + "-" * 80 + "\n"
            error_message += "Missing Placeholders:\n"
            error_message += "-" * 80 + "\n"
            error_message += (
                f"  The following {len(sorted_placeholders)} placeholder(s) were found in the\n"
                f"  application config but were not overridden in your YAML overlay:\n\n"
            )

            for idx, placeholder in enumerate(sorted_placeholders, 1):
                # Find where this placeholder appears in the config
                locations = NodeConfigLoader._find_placeholder_locations(config, placeholder)
                # Convert placeholder back to YAML key format for suggestion
                yaml_key_suggestion = NodeConfigLoader._placeholder_to_yaml_key(placeholder)

                error_message += f"  Missing Placeholder #{idx}:\n\n"
                error_message += f"    Placeholder:\n"
                error_message += f"      {placeholder}\n\n"
                if locations:
                    error_message += f"    Location(s) in JSON:\n"
                    for loc in locations[:3]:  # Show up to 3 locations
                        error_message += f"      key path: {loc}\n"
                    if len(locations) > 3:
                        error_message += f"      ... and {len(locations) - 3} more location(s)\n"
                error_message += f"\n    Expected config key:\n"
                error_message += f"      {yaml_key_suggestion}\n\n"
                error_message += f"    Add to your config file as:\n"
                error_message += f"      {yaml_key_suggestion}: <value>\n"
                if idx < len(sorted_placeholders):
                    error_message += "\n"

            error_message += "\n" + "-" * 80 + "\n"
            error_message += "To fix:\n"
            error_message += (
                "  1. Add the missing placeholder entries to your YAML overlay file, or\n"
                "  2. Remove the placeholders from the application config JSON file.\n"
            )
            error_message += "=" * 80

            raise ValueError(error_message)


class GrafanaDashboardConfigLoader(Config):
    def __init__(self, dashboard_file_path: str):
        self.dashboard_file_path = os.path.abspath(dashboard_file_path)
        self._validate()

    def _validate(self):
        self._validate_file(self.dashboard_file_path)

    def load(self) -> dict:
        return self._try_load_json(self.dashboard_file_path)


class GrafanaAlertRuleGroupConfigLoader(Config):
    def __init__(self, alerts_folder_path: str):
        self.alerts_folder_path = Path(alerts_folder_path)
        self._validate()

    def _validate(self):
        self._validate_directory(str(self.alerts_folder_path))

    def get_alert_files(self) -> List[Path]:
        """List all alert rule JSON files."""
        return list(self.alerts_folder_path.glob("*.json"))

    def load(self, alert_file_path: str) -> Dict[str, Any]:  # type: ignore[override]
        """Load a single alert rule JSON file."""
        return self._try_load_json(alert_file_path)
