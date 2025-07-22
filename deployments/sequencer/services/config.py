import json
import jsonschema
import os
from abc import ABC, abstractmethod
from pathlib import Path
from typing import Any, Dict, List, Optional, Union


class Config(ABC):
    @abstractmethod
    def load(self, file_path: Optional[Union[str, Path]] = None) -> Dict[str, Any]:
        pass

    @abstractmethod
    def _validate(self) -> None:
        pass

    def _validate_file(
        self, file_path: str, must_be_json: bool = True, must_be_readable: bool = True
    ) -> None:
        """Validate that a file path exists, is a file, and optionally is a JSON file and readable."""
        path = Path(file_path)
        if not path.exists():
            raise ValueError(f"File path {file_path} does not exist")
        if not path.is_file():
            raise ValueError(f"File path {file_path} is not a file")
        if must_be_json and not file_path.endswith(".json"):
            raise ValueError(f"File path {file_path} is not a JSON file")
        if must_be_readable and not os.access(file_path, os.R_OK):
            raise ValueError(f"File path {file_path} is not readable")

    def _validate_directory(self, dir_path: str) -> None:
        path = Path(dir_path)
        if not path.is_dir():
            raise ValueError(f"Directory path {dir_path} is not a directory")
        if not path.exists():
            raise ValueError(f"Directory path {dir_path} does not exist")
        if not os.access(dir_path, os.R_OK):
            raise ValueError(f"Directory path {dir_path} is not readable")

    def _try_load(self, file_path: Union[str, Path], file_description: str) -> Dict[str, Any]:
        try:
            with open(file_path, "r", encoding="utf-8") as f:
                value: Dict[str, Any] = json.load(f)
                return value
        except json.JSONDecodeError as e:
            raise ValueError(f"{file_description} file {file_path} is not valid JSON: {str(e)}")
        except FileNotFoundError as e:
            raise ValueError(f"{file_description} file {file_path} not found: {str(e)}")
        except PermissionError as e:
            raise ValueError(f"{file_description} file {file_path} is not readable: {str(e)}")


class DeploymentConfig(Config):
    SCHEMA_FILE = "./schemas/deployment_config_schema.json"

    def __init__(self, deployment_config_file: str):
        self.deployment_config_file_path = deployment_config_file
        self._validate()
        self._deployment_config_data = self.load()
        self._schema = self._load_schema()
        self._validate_schema()

    def _validate(self) -> None:
        self._validate_file(self.deployment_config_file_path)
        self._validate_file(self.SCHEMA_FILE)

    def _validate_schema(self) -> None:
        try:
            jsonschema.validate(instance=self._deployment_config_data, schema=self._schema)
        except jsonschema.ValidationError as e:
            raise ValueError(f"Invalid deployment config file: {e.message}")
        except jsonschema.SchemaError as e:
            raise ValueError(f"Invalid schema file: {e.message}")

    def _load_schema(self) -> Dict[str, Any]:
        return self._try_load(file_path=self.SCHEMA_FILE, file_description="Schema")

    def load(self, file_path: Optional[Union[str, Path]] = None) -> Dict[str, Any]:
        return self._try_load(
            file_path=self.deployment_config_file_path, file_description="Deployment config"
        )

    def get_application_config_subdir(self) -> Union[str, None]:
        return self._deployment_config_data.get("application_config_subdir")

    def get_services(self) -> List[Dict[str, Any]]:
        services: List[Dict[str, Any]] = self._deployment_config_data.get("services", [])
        return services


class SequencerConfig(Config):
    ROOT_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../../")

    def __init__(self, config_subdir: str, config_paths: List[str]):
        self.config_subdir = os.path.join(self.ROOT_DIR, config_subdir)
        self.config_paths = config_paths

    def _validate(self) -> None:
        self._validate_directory(self.config_subdir)
        for config_path in self.config_paths:
            self._validate_file(os.path.join(self.config_subdir, config_path))

    def load(self, file_path: Optional[Union[str, Path]] = None) -> Dict[str, Any]:
        result = {}
        for config_path in self.config_paths:
            path = os.path.join(self.config_subdir, config_path)
            data = self._try_load(file_path=path, file_description="Config")
            result.update(data)  # later values overwrite previous
        return result


class GrafanaDashboardConfig(Config):
    def __init__(self, dashboard_file_path: str):
        self.dashboard_file_path = os.path.abspath(dashboard_file_path)
        self._validate()

    def _validate(self) -> None:
        self._validate_file(self.dashboard_file_path)

    def load(self, file_path: Optional[Union[str, Path]] = None) -> Dict[str, Any]:
        return self._try_load(file_path=self.dashboard_file_path, file_description="Dashboard")


class GrafanaAlertRuleGroupConfig(Config):
    def __init__(self, alerts_folder_path: str):
        self.alerts_folder_path = Path(alerts_folder_path)
        self._validate()

    def _validate(self) -> None:
        self._validate_directory(str(self.alerts_folder_path))
        for file in self.get_alert_files():
            self._validate_file(str(file))

    def get_alert_files(self) -> List[Path]:
        return list(self.alerts_folder_path.glob("*.json"))

    def load(self, file_path: Optional[Union[str, Path]] = None) -> Dict[str, Any]:
        assert file_path is not None, "File path must be provided for loading alert rule group."
        return self._try_load(file_path=file_path, file_description="Alert")
