import dataclasses
import typing


from services.config import SequencerConfig


@dataclasses.dataclass
class ServiceTopology:
    config: SequencerConfig
    image: str
    domain: str
    replicas: int
    autoscale: bool
    ingress: typing.Optional[dict[any, any]]
    tolerations: typing.Optional[list[str]]
    storage: typing.Optional[int]
    resources: typing.Optional[dict[str, dict[str, int]]]
    external_secret: typing.Optional[dict[str, str]]
