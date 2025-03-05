import dataclasses
import typing
import json

from jsonschema import validate, ValidationError

from services import objects


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

    def get_services(self):
        return [service for service in self._deployment_config_data.get("services", [])]


@dataclasses.dataclass
class ServiceTopology:
    config: typing.Optional[objects.Config]
    image: str
    ingress: bool
    autoscale: bool
    storage: int | None


class SequencerDev(ServiceTopology):
    pass


class SequencerProd(SequencerDev):
    pass
