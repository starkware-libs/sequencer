import base64
import json
import os
from pathlib import Path

from imports import k8s
from src.constructs.base import BaseConstruct


class SecretConstruct(BaseConstruct):
    def __init__(
        self,
        scope,
        id: str,
        service_config,
        labels,
        monitoring_endpoint_port,
    ):
        super().__init__(
            scope,
            id,
            service_config,
            labels,
            monitoring_endpoint_port,
        )

        self.secret = self._create_secret()

    def _load_secret_file(self) -> dict:
        """Load secret content from file if file path is specified."""
        if not self.service_config.secret.file:
            return {}

        # Resolve file path relative to project root (same as NodeConfigLoader)
        root_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../../../")
        file_path = os.path.join(root_dir, self.service_config.secret.file)

        # Validate file exists and is readable
        path = Path(file_path)
        if not path.exists():
            raise ValueError(f"Secret file '{self.service_config.secret.file}' does not exist")
        if not path.is_file():
            raise ValueError(f"Secret file '{self.service_config.secret.file}' is not a file")
        if not os.access(file_path, os.R_OK):
            raise ValueError(f"Secret file '{self.service_config.secret.file}' is not readable")

        if not file_path.endswith(".json"):
            raise ValueError(f"Secret file '{self.service_config.secret.file}' must be a JSON file")

        try:
            with open(file_path, "r", encoding="utf-8") as f:
                secret_data = json.load(f)
        except json.JSONDecodeError as e:
            raise ValueError(
                f"Invalid JSON in secret file '{self.service_config.secret.file}': {e}"
            )

        # Convert to JSON string and return as dict with secrets.json key
        return {"secrets.json": json.dumps(secret_data, indent=2)}

    def _create_secret(self) -> k8s.KubeSecret:
        # Merge secret labels with common labels
        secret_labels = {**self.labels, **self.service_config.secret.labels}

        # Load secret from file if specified
        file_string_data = self._load_secret_file()

        # Merge file content with existing stringData (file takes precedence)
        string_data = {**file_string_data, **self.service_config.secret.stringData}

        # Encode stringData to base64 and add to data field
        data = {}
        if string_data:
            for key, value in string_data.items():
                data[key] = base64.b64encode(value.encode("utf-8")).decode("utf-8")

        # Add any existing data (already base64 encoded)
        data.update(self.service_config.secret.data)

        if not data:
            raise ValueError("Secret must have data, stringData, or file with at least one key")

        return k8s.KubeSecret(
            self,
            "secret",
            metadata=k8s.ObjectMeta(
                name=self.service_config.secret.name
                or f"sequencer-{self.service_config.name}-secret",
                labels=secret_labels,
                annotations=self.service_config.secret.annotations,
            ),
            type=self.service_config.secret.type,
            data=data,
            immutable=self.service_config.secret.immutable,
        )
