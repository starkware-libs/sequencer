import dataclasses
import typing


from services.config import ServiceConfig


@dataclasses.dataclass
class ServiceTopology:
    config: ServiceConfig
    image: str
    controller: str
    replicas: int
    autoscale: bool
    ingress: typing.Optional[dict[any, any]]
    toleration: typing.Optional[str]
    storage: typing.Optional[int]
    resources: typing.Optional[dict[str, dict[str, int]]]
    external_secret: typing.Optional[dict[str, str]]
