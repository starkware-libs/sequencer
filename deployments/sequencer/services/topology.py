import dataclasses
import typing


from services.config import SequencerConfig


@dataclasses.dataclass
class ServiceTopology:
    config: typing.Optional[SequencerConfig]
    image: str
    ingress: bool
    replicas: int
    autoscale: bool
    storage: int | None
