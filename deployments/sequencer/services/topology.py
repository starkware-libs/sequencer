import dataclasses
from typing import Any, Dict, Optional, Union

from sequencer.services.config import SequencerConfig


@dataclasses.dataclass
class ServiceTopology:
    config: SequencerConfig
    image: str
    controller: str
    replicas: int
    autoscale: bool
    anti_affinity: bool
    k8s_service_config: Dict[str, Union[str, bool]]
    ingress: Optional[Dict[Any, Any]]
    toleration: Optional[str]
    storage: Optional[int]
    resources: Optional[Dict[str, Dict[str, int]]]
    external_secret: Optional[Dict[str, str]]
