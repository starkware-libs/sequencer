import dataclasses
from typing import Optional, List, Dict, Any

from services.config import SequencerConfig


@dataclasses.dataclass
class ServiceTopology:
    config: SequencerConfig
    image: str
    controller: str
    replicas: int
    autoscale: bool
    ports: Optional[Dict[str, int]]
    ingress: Optional[Dict[Any, Any]]
    toleration: Optional[str]
    storage: Optional[int]
    resources: Optional[Dict[str, Dict[str, int]]]
    external_secret: Optional[Dict[str, str]]
