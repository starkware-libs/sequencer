import dataclasses

from typing import Optional, Dict, Any, TypedDict
from enum import Enum


@dataclasses.dataclass
class Probe:
    port: int | str
    path: str
    period_seconds: int
    failure_threshold: int
    timeout_seconds: int

    def __post_init__(self):
        assert not isinstance(self.port, (bool)), \
            "Port must be of type int or str, not bool."


@dataclasses.dataclass
class HealthCheck:
    startup_probe: Optional[Probe] = None
    readiness_probe: Optional[Probe] = None
    liveness_probe: Optional[Probe] = None


@dataclasses.dataclass
class ServiceType(Enum):
    CLUSTER_IP = "ClusterIP"
    LOAD_BALANCER = "LoadBalancer"
    NODE_PORT = "NodePort"


@dataclasses.dataclass
class PersistentVolumeClaim:
    storage_class_name: str | None = None
    access_modes: list[str] | None = None
    volume_mode: str | None = None
    storage: str | None = None
    read_only: bool = True
    mount_path: str | None = None


@dataclasses.dataclass
class Config():
    schema: Dict[Any, Any]
    config: Dict[Any, Any]
    mount_path: str

    def get(self):
        return self.config

    def validate(self):
        pass


@dataclasses.dataclass
class PortMappings(TypedDict):
    name: str
    port: int
    container_port: int