import os
import json
import jsonschema

from services.objects import Config


ROOT_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), '../../../')
CONFIG_DIR = os.path.join(ROOT_DIR, 'config/sequencer/')
CONFIG = os.getenv("CONFIG", None)


class SequencerDevConfig(Config):
    def __init__(self, mount_path: str):
        super().__init__(
            schema=json.loads(open(os.path.join(CONFIG_DIR, 'default_config.json'), 'r').read()),
            config=json.loads(open(os.path.join(CONFIG_DIR, 'presets', 'config.json'), 'r').read()) if not CONFIG else json.loads(open(os.path.abspath(CONFIG)).read()),
            mount_path = mount_path
        )

    def validate(self):
        jsonschema.validate(self.config, schema=self.schema)
