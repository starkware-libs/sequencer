import dataclasses
import typing

from services.config import SequencerConfig


@dataclasses.dataclass
class ServiceTopology:
    config: SequencerConfig
    image: str
    controller: str
    replicas: int
    autoscale: bool
    anti_affinity: bool
    k8s_service_config: dict[str, typing.Union[str, bool]]
    ingress: typing.Optional[dict[any, any]]
    toleration: typing.Optional[str]
    storage: typing.Optional[int]
    resources: typing.Optional[dict[str, dict[str, int]]]
    external_secret: typing.Optional[dict[str, str]]
