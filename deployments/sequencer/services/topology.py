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
    storage: typing.Optional[int]
    resources: typing.Optional[dict[str, typing.Any]]
