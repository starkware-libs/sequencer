from dataclasses import dataclass
from typing import Optional, Dict, Union
from imports import k8s


@dataclass
class Probe:
    port: Union[str, int]
    path: str
    period_seconds: int
    failure_threshold: int
    timeout_seconds: int = 5

@dataclass
class HealthCheck:
    startup_probe_field: k8s.Probe
    readiness_probe_field: k8s.Probe
    liveness_probe_field: k8s.Probe

    def __init__(
        self,
        startup_probe: Probe, # TODO: Rename.
        liveness_probe: Probe,
        readiness_probe: Probe
    ):
        self.startup_probe = k8s.Probe(
            http_get=k8s.HttpGetAction(port=self._create_port(startup_probe.port), path=startup_probe.path),
            period_seconds=startup_probe.period_seconds, failure_threshold=startup_probe.failure_threshold, timeout_seconds=startup_probe.timeout_seconds
        )
        self.readiness_probe = k8s.Probe(
            http_get=k8s.HttpGetAction(port=self._create_port(readiness_probe.port), path=readiness_probe.path),
            period_seconds=readiness_probe.period_seconds, failure_threshold=readiness_probe.failure_threshold, timeout_seconds=readiness_probe.timeout_seconds
        )
        self.liveness_probe = k8s.Probe(
            http_get=k8s.HttpGetAction(port=self._create_port(liveness_probe.port), path=liveness_probe.path),
            period_seconds=liveness_probe.period_seconds, failure_threshold=liveness_probe.failure_threshold, timeout_seconds=liveness_probe.timeout_seconds
        )

    def _create_port(port: Union[str, int]):
        return k8s.IntOrString.from_string(port) if isinstance(port, str) else k8s.IntOrString.from_number(port)
