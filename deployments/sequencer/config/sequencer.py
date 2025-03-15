import os
import json
from typing import Dict, Any


class SequencerConfig:
    ROOT_DIR: os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../../")

    def __init__(self, config_subdir: str, config_path: str):
        self.config_subdir = os.path.join(self.ROOT_DIR, config_subdir)
        self.config_path = os.path.join(self.config_subdir, config_path)

    def get_config(self) -> Dict[Any, Any]:
        with open(self.config_path) as config_file:
            return json.loads(config_file.read())

    def validate(self):
        pass
