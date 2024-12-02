import dataclasses
from typing import Optional, List, Dict, Any, Mapping, Sequence

from imports import k8s
from services import const


def to_quantity(value: str | int | float) -> k8s.Quantity:
    if isinstance(value, str):
        return k8s.Quantity.from_string(value)
    elif isinstance(value, (int, float)):
        return k8s.Quantity.from_number(value)
    else:
        raise ValueError("Value must be of type int, float or str.")


def to_int_or_string(value: str | int | float):
    if isinstance(value, str):
        return k8s.IntOrString.from_string(value)
    elif isinstance(value, (int, float)):
        return k8s.IntOrString.from_number(value)
    else:
        raise ValueError("Value must be of type int, float or str.")


@dataclasses.dataclass
class HttpProbe:
    port: int | str
    path: str
    period_seconds: int
    failure_threshold: int
    timeout_seconds: int

    def __post_init__(self):
        assert not isinstance(self.port, bool), "Port must be of type int or str, not bool."

    def to_k8s(self) -> k8s.Probe:
        return k8s.Probe(
            http_get=k8s.HttpGetAction(
                port=to_int_or_string(self.port),
                path=self.path,
            ),
            period_seconds=self.period_seconds,
            failure_threshold=self.failure_threshold,
            timeout_seconds=self.timeout_seconds
        )

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
    schema: Dict[Any, Any]
    config: Dict[Any, Any]
    mount_path: str

    def get(self):
        return self.config

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

    def to_k8s(self) -> k8s.VolumeMount:
        return k8s.VolumeMount(
            name=self.name,
            mount_path=self.mount_path,
            read_only=self.read_only
        )


@dataclasses.dataclass
class ConfigMapVolume:
    name: str


@dataclasses.dataclass
class PvcVolume:
    name: str
    read_only: bool


@dataclasses.dataclass
class ContainerResources:
    requests_cpu: str | int
    requests_memory: str
    limits_cpu: str | int
    limits_memory: str

    def to_k8s(self) -> k8s.ResourceRequirements:
        return k8s.ResourceRequirements(
            requests={
                "cpu": to_quantity(self.requests_cpu),
                "memory": to_quantity(self.requests_memory),
            },
            limits={
                "cpu": to_quantity(self.limits_cpu),
                "memory": to_quantity(self.limits_memory),
            }
        )


@dataclasses.dataclass
class ContainerPort:
    port: int
    name: Optional[str] = None
    protocol: Optional[str] = None

    def to_k8s(self) -> k8s.ContainerPort:
        return k8s.ContainerPort(
            container_port=self.port,
            name=self.name,
            protocol=self.protocol
        )


@dataclasses.dataclass
class Container:
    name: str
    image: str
    ports: Sequence[ContainerPort]
    resources: ContainerResources
    startup_probe: Optional[HttpProbe]
    readiness_probe: Optional[HttpProbe]
    liveness_probe: Optional[HttpProbe]
    volume_mounts: Sequence[VolumeMount]
    args: Optional[List[str]] = None
    command: Optional[List[str]] = None


@dataclasses.dataclass
class Deployment:
    replicas: int
    annotations: Mapping[str, str] | None
    containers: Sequence[Container] | None
    configmap_volumes: Sequence[ConfigMapVolume] | None
    pvc_volumes: Sequence[PvcVolume] | None
