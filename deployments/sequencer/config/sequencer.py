import typing
import os
import json
import jsonschema


from services.objects import Config


def load_config(config_path):
    root_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), '../../../')
    config_dir = os.path.join(root_dir, 'config/sequencer/')
    
    return json.loads(
        open(os.path.join(config_dir, config_path), 'r').read()
    )

class SequencerDevConfig(Config):
    def __init__(self, mount_path: str, custom_config_path: typing.Optional[str]):
        super().__init__(
            schema=load_config(config_path='default_config.json'),
            config=load_config(config_path='presets/config.json') if not custom_config_path else json.loads(open(os.path.abspath(custom_config_path)).read()),
            mount_path=mount_path
        )


    def validate(self):
        jsonschema.validate(self.config, schema=self.schema)
