import json
import os
import yaml
from abc import ABC, abstractmethod
from pathlib import Path
from typing import Any, Dict, List, Optional

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
        raw = self._try_load_yaml(str(self.common_config_path))
        validated = CommonConfig.model_validate(raw).model_dump(exclude_none=True)
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
        config_list: List[str] = self._try_load_json(file_path=config_list_full_path)

        # Validate that it's a list of strings
        if not isinstance(config_list, list):
            raise ValueError(
                f"Config list JSON file '{self.config_list_json_path}' must contain a JSON array. Got: {type(config_list)}"
            )
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

        Assumes all placeholders in the application config use snake_case (e.g., COMPONENTS_SIERRA_COMPILER_URL).
        The YAML key should match this format (e.g., components_sierra_compiler_url).

        Args:
            yaml_key: The YAML key in snake_case (e.g., 'chain_id', 'components_sierra_compiler_url')

        Returns:
            The full placeholder format (e.g., '$$$_CHAIN_ID_$$$', '$$$_COMPONENTS_SIERRA_COMPILER_URL_$$$')

        Examples:
            'chain_id' -> '$$$_CHAIN_ID_$$$'
            'components_sierra_compiler_url' -> '$$$_COMPONENTS_SIERRA_COMPILER_URL_$$$'
            'consensus_manager_config_network_config_advertised_multiaddr' -> '$$$_CONSENSUS_MANAGER_CONFIG_NETWORK_CONFIG_ADVERTISED_MULTIADDR_$$$'
        """
        # Simple: just uppercase the key and wrap with placeholder markers
        placeholder = yaml_key.upper()
        return f"$$$_{placeholder}_$$$"

    @staticmethod
    def apply_sequencer_overrides(
        merged_json_config: dict, sequencer_config: Dict[str, Any], service_name: str = "unknown"
    ) -> dict:
        """Apply sequencerConfig overrides from YAML to merged JSON config.

        Overrides are applied by placeholder value, not by JSON key. This makes the
        deployment resilient to JSON key changes as long as the placeholder values remain the same.

        YAML keys are simplified (e.g., 'chain_id') and automatically converted to
        placeholder format (e.g., '$$$_CHAIN_ID_$$$') for matching.

        Args:
            merged_json_config: The merged JSON config dictionary from all config files
            sequencer_config: Dictionary from YAML with simplified keys:
                {
                    'chain_id': '$$$_CHAIN_ID_$$$',
                    'starknet_url': '$$$_STARKNET_URL_$$$'
                }
                These are converted to placeholder format for matching.

        Returns:
            Updated config dictionary with overrides applied

        Raises:
            ValueError: If any YAML key doesn't match any placeholder in the config

        Examples:
            JSON: {'chain_id': '$$$_CHAIN_ID_$$$', 'some_other_key': '$$$_CHAIN_ID_$$$'}
            YAML: {'chain_id': '123'}
            Result: {'chain_id': '123', 'some_other_key': '123'}
        """
        # Step 1: Normalize all placeholders in the merged config (replace - with _)
        result = NodeConfigLoader._normalize_placeholders_in_config(merged_json_config)

        # Step 2: Validate that all YAML keys match existing placeholders
        unmatched_keys = []
        for yaml_key, replacement_value in sequencer_config.items():
            # Convert YAML key to placeholder format and normalize it
            placeholder = NodeConfigLoader._yaml_key_to_placeholder(yaml_key)
            placeholder = NodeConfigLoader._normalize_placeholder(placeholder)
            # Check if placeholder exists in the normalized config
            exists = NodeConfigLoader._placeholder_exists(result, placeholder)
            if not exists:
                unmatched_keys.append((yaml_key, placeholder))

        # Raise error if any keys don't match
        if unmatched_keys:
            error_messages = []
            for yaml_key, placeholder in unmatched_keys:
                error_messages.append(
                    f"  - YAML key '{yaml_key}' (maps to placeholder '{placeholder}') "
                    f"does not match any placeholder in the '{service_name}' service application config."
                )
            raise ValueError(
                f"Invalid sequencerConfig override keys found for service '{service_name}'. "
                f"The following keys do not match any placeholder in the application config:\n"
                + "\n".join(error_messages)
            )

        # Step 3: Apply overrides
        for yaml_key, replacement_value in sequencer_config.items():
            # Convert YAML key to placeholder format and normalize it
            placeholder = NodeConfigLoader._yaml_key_to_placeholder(yaml_key)
            placeholder = NodeConfigLoader._normalize_placeholder(placeholder)
            # Replace the placeholder value wherever it appears in the config
            result = NodeConfigLoader._replace_placeholder_value(
                result, placeholder, replacement_value
            )

        # Re-sort after modifications
        return dict[Any, Any](sorted(result.items()))


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

    def load(self, alert_file_path: str) -> Dict[str, Any]:
        """Load a single alert rule JSON file."""
        return self._try_load_json(alert_file_path)
