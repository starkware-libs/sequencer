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
        if self.common_config_path and not self.common_config_path.exists():
            raise ValueError(f"Common config path {self.common_config_path} does not exist")

    def _load_service_configs_from_dir(self) -> List[ServiceConfig]:
        """Load and validate each service YAML file in the directory."""
        validated_configs = []
        for fname in os.listdir(self.configs_dir_path):
            if not fname.endswith((".yaml", ".yml")):
                continue
            file_path = self.configs_dir_path / fname
            raw_config = self._try_load_yaml(str(file_path))
            validated_config = ServiceConfig.model_validate(raw_config)
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


class GrafanaDashboardConfig(Config):
    def __init__(self, dashboard_file_path: str):
        self.dashboard_file_path = os.path.abspath(dashboard_file_path)
        self._validate()

    def _validate(self):
        self._validate_file(self.dashboard_file_path)

    def load(self) -> dict:
        return self._try_load_json(self.dashboard_file_path)


class GrafanaAlertRuleGroupConfig(Config):
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
