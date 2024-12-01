import os
import json
import jsonschema

from services.objects import Config


ROOT_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../../")
CONFIG_DIR = os.path.join(ROOT_DIR, "config/sequencer/")


class SequencerDevConfig(Config):
    def __init__(self, mount_path: str, config_file_path: str = ""):
        super().__init__(
            schema=json.loads(open(os.path.join(CONFIG_DIR, "default_config.json"), "r").read()),
            config=json.loads(open(os.path.join(CONFIG_DIR, "presets", "config.json"), "r").read())
            if not config_file_path
            else json.loads(open(os.path.abspath(config_file_path)).read()),
            mount_path=mount_path,
        )

    def validate(self):
        jsonschema.validate(self.config, schema=self.schema)
