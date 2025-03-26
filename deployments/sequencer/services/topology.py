import dataclasses
import typing


from services.config import SequencerConfig


@dataclasses.dataclass
class ServiceTopology:
    config: SequencerConfig
    image: str
    ingress: bool
    replicas: int
    autoscale: bool
    storage: typing.Optional[int]
    resources: typing.Optional[dict[str, dict[str, int]]]
