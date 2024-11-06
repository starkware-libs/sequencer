from typing import Dict, Any
import os
import json
import jsonschema

ROOT_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), '../../../')
CONFIG_DIR = os.path.join(ROOT_DIR, 'config/papyrus/')


class Config():
    def __init__(self, schema: Dict[Any, Any], config: Dict[Any, Any]):
        self.schema = schema
        self.config = config

    def get(self):
        return self.config

    def validate(self):
        pass


class SequencerDevConfig(Config):
    def __init__(self):
        super().__init__(
            schema=json.loads(open(os.path.join(CONFIG_DIR, 'default_config.json'), 'r').read()),
            config=json.loads(open(os.path.join(CONFIG_DIR, 'presets', 'sepolia_testnet.json'), 'r').read())
        )

    def validate(self):
        jsonschema.validate(self.config, schema=self.schema)
