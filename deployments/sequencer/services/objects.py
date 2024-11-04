import dataclasses

from typing import Union
from imports import k8s


@dataclasses.dataclass
class Probe:
    port: Union[str, int] = "http"
    path: str = "/"
    period_seconds: int
    failure_threshold: int
    timeout_seconds: int = 5

@dataclasses.dataclass
class HealthCheck:
    startup_probe: Probe
    readiness_probe: Probe
    liveness_probe: Probe

    def __post_init__(self):
        self.configure_probes()

    def _create_port(self, port: Union[str, int]):
        return k8s.IntOrString.from_string(port) if isinstance(port, str) else k8s.IntOrString.from_number(port)

    def _create_http_get_action(self, probe: Probe):
        return k8s.HttpGetAction(port=self._create_port(probe.port), path=probe.path)

    def _create_k8s_probe(self, probe: Probe):
        return k8s.Probe(
            http_get=self._create_http_get_action(probe),
            period_seconds=probe.period_seconds,
            failure_threshold=probe.failure_threshold,
            timeout_seconds=probe.timeout_seconds
        )

    def configure_probes(self):
        self.startup_probe = self._create_k8s_probe(self.startup_probe)
        self.readiness_probe = self._create_k8s_probe(self.readiness_probe)
        self.liveness_probe = self._create_k8s_probe(self.liveness_probe)
