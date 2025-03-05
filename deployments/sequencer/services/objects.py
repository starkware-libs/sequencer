import dataclasses
from typing import Dict, Any


@dataclasses.dataclass
class Config:
    global_config: Dict[Any, Any]
    config: Dict[Any, Any]

    def _merged_config(self) -> Dict[Any, Any]:
        _config = self.global_config.copy()
        _config.update(self.config)
        return _config

    def get_merged_config(self) -> Dict[Any, Any]:
        return self._merged_config()

    def get_config(self) -> Dict[Any, Any]:
        return self.config

    def validate(self):
        pass
