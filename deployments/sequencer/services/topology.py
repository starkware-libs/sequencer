import dataclasses
import typing


from services.config import SequencerConfig


@dataclasses.dataclass
class ServiceTopology:
    config: SequencerConfig
    image: str
    component: str
    controller_type: str
    replicas: int
    autoscale: bool
    ingress: typing.Optional[dict[str, any]]
    toleration: typing.Optional[str]
    storage: typing.Optional[int]
    resources: typing.Optional[dict[str, dict[str, int]]]
    external_secret: typing.Optional[dict[str, str]]
