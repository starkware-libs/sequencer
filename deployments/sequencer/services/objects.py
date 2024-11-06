import dataclasses

from typing import Optional, Union
from imports import k8s
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

    def to_k8s_probe(self) -> k8s.Probe:
        k8s_port = (
            k8s.IntOrString.from_string(self.port)
            if isinstance(self.port, str)
            else k8s.IntOrString.from_number(self.port)
        )
        k8s_http_get = k8s.HttpGetAction(port=k8s_port, path=self.path)
        return k8s.Probe(
            http_get=k8s_http_get,
            period_seconds=self.period_seconds,
            failure_threshold=self.failure_threshold,
            timeout_seconds=self.timeout_seconds,
        )


@dataclasses.dataclass
class HealthCheck:
    startup_probe: Optional[k8s.Probe] = None
    readiness_probe: Optional[k8s.Probe] = None
    liveness_probe: Optional[k8s.Probe] = None

    def __init__(self, startup_probe: Optional[Probe] = None, readiness_probe: Optional[Probe] = None, liveness_probe: Optional[Probe] = None):
        self.startup_probe = startup_probe.to_k8s_probe() if startup_probe is not None else None
        self.readiness_probe = readiness_probe.to_k8s_probe() if readiness_probe is not None else None
        self.liveness_probe = liveness_probe.to_k8s_probe() if liveness_probe is not None else None


@dataclasses.dataclass
class ServiceType(Enum):
    CLUSTER_IP = "ClusterIP"
    LOAD_BALANCER = "LoadBalancer"
    NODE_PORT = "NodePort"