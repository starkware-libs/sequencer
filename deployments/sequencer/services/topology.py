import dataclasses
import typing
import json

from jsonschema import validate, ValidationError

from services import objects


class Preset:
    SCHEMA_FILE = "preset_schema.json"

    def __init__(self, preset_file: str):
        self.preset_file = preset_file
        self._preset_data = self._read_preset_file()
        self._schema = self._load_schema()
        self._validate_preset()

    def _validate_preset(self):
        try:
            validate(instance=self._preset_data, schema=self._schema)
        except ValidationError as e:
            raise ValueError(f"Invalid preset file: {e.message}")

    def _load_schema(self):
        with open(self.SCHEMA_FILE) as f:
            return json.load(f)

    def _read_preset_file(self):
        with open(self.preset_file) as f:
            return json.loads(f.read())

    def get_chain_id(self):
        return self._preset_data.get("chain_id")

    def get_image(self):
        return self._preset_data.get("image")

    def get_services(self):
        return [service for service in self._preset_data.get("services", [])]


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
