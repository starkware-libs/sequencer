import json
import os
from typing import Dict, Any, List
from jsonschema import validate, ValidationError


class DeploymentConfig:
    SCHEMA_FILE = "deployment_config_schema.json"

    def __init__(self, deployment_config_file: str):
        self.deployment_config_file_path = deployment_config_file
        self._deployment_config_data = self._read_deployment_config_file()
        self._schema = self._load_schema()
        self._validate_deployment_config()

    def _validate_deployment_config(self):
        try:
            validate(instance=self._deployment_config_data, schema=self._schema)
        except ValidationError as e:
            raise ValueError(f"Invalid deployment config file: {e.message}")

    def _load_schema(self):
        with open(self.SCHEMA_FILE) as f:
            return json.load(f)

    def _read_deployment_config_file(self):
        with open(self.deployment_config_file_path) as f:
            return json.loads(f.read())

    def get_nodes(self):
        return self._deployment_config_data.get("nodes", [])

    def get_chain_id(self):
        return self._deployment_config_data.get("chain_id")

    def get_application_config_subdir(self, index: int):
        return self._deployment_config_data["nodes"][index].get("application_config_subdir")

    def get_services(self, index: int):
        return [
            service for service in self._deployment_config_data["nodes"][index].get("services", [])
        ]


class ServiceConfig:
    ROOT_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../../")

    def __init__(self, config_subdir: str, config_paths: List[str]):
        self.config_subdir = os.path.join(self.ROOT_DIR, config_subdir)
        self.config_paths = config_paths

    def get_config(self) -> Dict[Any, Any]:
        result = {}
        for config_path in self.config_paths:
            path = os.path.join(self.config_subdir, config_path)
            with open(path, "r", encoding="utf-8") as f:
                data = json.load(f)
                if not isinstance(data, dict):
                    raise ValueError(f"File {path} does not contain a JSON object")
                result.update(data)  # later values overwrite previous

        return result

    def validate(self):
        pass
