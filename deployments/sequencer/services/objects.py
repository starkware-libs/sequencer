import dataclasses
from typing import Optional, List, Dict, Any, Mapping, Sequence
from services import const


@dataclasses.dataclass
class Probe:
    port: int | str
    path: str
    period_seconds: int
    failure_threshold: int
    timeout_seconds: int

    def __post_init__(self):
        assert not isinstance(self.port, (bool)), "Port must be of type int or str, not bool."


@dataclasses.dataclass
class PortMapping:
    name: str
    port: int
    container_port: int


@dataclasses.dataclass
class Service:
    type: Optional[const.ServiceType]
    selector: Mapping[str, str]
    ports: Sequence[PortMapping]


@dataclasses.dataclass
class PersistentVolumeClaim:
    storage_class_name: str | None
    access_modes: list[str] | None
    volume_mode: str | None
    storage: str | None
    read_only: bool | None
    mount_path: str | None


@dataclasses.dataclass
class Config:
    global_config: Dict[Any, Any]
    config: Dict[Any, Any]
    mount_path: str

    def _merged_config(self) -> Dict[Any, Any]:
        _config = self.global_config.copy()
        _config.update(self.config)
        return _config

    def get_config(self):
        return self._merged_config()

    def validate(self):
        pass


@dataclasses.dataclass
class IngressRuleHttpPath:
    path: Optional[str]
    path_type: str
    backend_service_name: str
    backend_service_port_number: int
    backend_service_port_name: Optional[str] = None


@dataclasses.dataclass
class IngressRule:
    host: str
    paths: Sequence[IngressRuleHttpPath]


@dataclasses.dataclass
class IngressTls:
    hosts: Sequence[str] | None = None
    secret_name: str | None = None


@dataclasses.dataclass
class Ingress:
    annotations: Mapping[str, str] | None
    class_name: str | None
    rules: Sequence[IngressRule] | None
    tls: Sequence[IngressTls] | None


@dataclasses.dataclass
class VolumeMount:
    name: str
    mount_path: str
    read_only: bool


@dataclasses.dataclass
class ConfigMapVolume:
    name: str


@dataclasses.dataclass
class PvcVolume:
    name: str
    read_only: bool


@dataclasses.dataclass
class ContainerPort:
    container_port: int


@dataclasses.dataclass
class Container:
    name: str
    image: str
    args: List[str]
    ports: Sequence[ContainerPort]
    startup_probe: Optional[Probe]
    readiness_probe: Optional[Probe]
    liveness_probe: Optional[Probe]
    volume_mounts: Sequence[VolumeMount]


@dataclasses.dataclass
class Deployment:
    replicas: int
    annotations: Mapping[str, str] | None
    containers: Sequence[Container] | None
    configmap_volumes: Sequence[ConfigMapVolume] | None
    pvc_volumes: Sequence[PvcVolume] | None
