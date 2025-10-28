import json
import os
import yaml


class Config:
    def __init__(self, file_path: str):
        self.file_path = os.path.expanduser(os.path.expandvars(file_path))

    def _resolve_path(self) -> str:
        if os.path.exists(self.file_path):
            return self.file_path

        # Try relative to the script directory
        script_dir = os.path.dirname(os.path.abspath(__file__))
        candidate = os.path.join(script_dir, os.path.basename(self.file_path))
        if os.path.exists(candidate):
            return candidate

        raise FileNotFoundError(f"Config file not found: {self.file_path}")

    def _validate(self) -> str:
        path = self._resolve_path()
        if not os.path.isfile(path):
            raise ValueError(f"Path {path} is not a file")
        if not os.access(path, os.R_OK):
            raise PermissionError(f"File {path} is not readable")
        return path

    def load(self) -> dict:
        raise NotImplementedError("Subclasses must implement load()")


class JsonConfig(Config):
    def load(self) -> dict:
        path = self._validate()
        if not path.endswith(".json"):
            raise ValueError(f"File {path} is not a JSON file")
        with open(path, "r", encoding="utf-8") as f:
            try:
                return json.load(f)
            except json.JSONDecodeError as e:
                raise ValueError(f"Invalid JSON in {path}: {e}")


class YamlConfig(Config):
    def load(self) -> dict:
        path = self._validate()
        if not (path.endswith(".yaml") or path.endswith(".yml")):
            raise ValueError(f"File {path} is not a YAML file")
        with open(path, "r", encoding="utf-8") as f:
            try:
                return yaml.safe_load(f)
            except yaml.YAMLError as e:
                raise ValueError(f"Invalid YAML in {path}: {e}")
