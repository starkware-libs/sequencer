import json
import os
from typing import Dict, Any
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

    def get_chain_id(self):
        return self._deployment_config_data.get("chain_id")

    def get_image(self):
        return self._deployment_config_data.get("image")

    def get_application_config_subdir(self):
        return self._deployment_config_data.get("application_config_subdir")

    def get_services(self):
        return [service for service in self._deployment_config_data.get("services", [])]


class SequencerConfig:
    ROOT_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../../")

    def __init__(self, config_subdir: str, config_path: str):
        self.config_subdir = os.path.join(self.ROOT_DIR, config_subdir)
        self.config_path = os.path.join(self.config_subdir, config_path)

    def get_config(self) -> Dict[Any, Any]:
        with open(self.config_path) as config_file:
            return json.loads(config_file.read())

    def validate(self):
        pass
