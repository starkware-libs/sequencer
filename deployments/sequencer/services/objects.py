import dataclasses

from typing import Optional, Union
from enum import Enum


@dataclasses.dataclass
class Probe:
    port: Union[int, str]
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
    storage_class_name: str = None
    access_modes: list[str] = None
    volume_mode: str = None
    storage: str = None
    read_only: bool = True
    mount_path: str = None


