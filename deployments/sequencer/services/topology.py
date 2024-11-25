import dataclasses
import typing


from services import (
    objects,
    topology_helpers
)


@dataclasses.dataclass
class ServiceTopology:
    name: str
    namespace: str
    deployment: typing.Optional[objects.Deployment] = dataclasses.field(default_factory=topology_helpers.get_deployment)
    config: typing.Optional[objects.Config] = dataclasses.field(default_factory=topology_helpers.get_config)
    service: typing.Optional[objects.Service] = dataclasses.field(default_factory=topology_helpers.get_service)
    port_mappings: typing.Optional[typing.Sequence[objects.PortMapping]] = dataclasses.field(default_factory=topology_helpers.get_port_mappings)
    pvc: typing.Optional[objects.PersistentVolumeClaim] = dataclasses.field(default_factory=topology_helpers.get_pvc)
    ingress: typing.Optional[objects.Ingress] = dataclasses.field(default_factory=topology_helpers.get_ingress)


class SequencerDev(ServiceTopology):
    pass

class SequencerProd(SequencerDev):
    pass
