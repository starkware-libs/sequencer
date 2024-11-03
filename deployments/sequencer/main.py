#!/usr/bin/env python3

from constructs import Construct # type: ignore
from cdk8s import App, Chart # type: ignore
from typing import Dict, Any
from services.service import Service
import dataclasses

from config.sequencer import Config, SequencerDevConfig

from services.objects import Probe, HealthCheck


@dataclasses.dataclass
class SystemStructure:
    topology: str = "mesh"
    replicas: str = "2"
    size: str = "small"
    config: Config = SequencerDevConfig()
    startup_probe: Probe = Probe(
        port="http",
        path="/",
        period_seconds=5,
        failure_threshold=5,
        timeout_seconds=10
    ),
    readiness_probe: Probe = Probe(
        port="http",
        path="/",
        period_seconds=5,
        failure_threshold=5,
        timeout_seconds=10
    ),
    liveness_probe: Probe = Probe(
        port="http",
        path="/",
        period_seconds=5,
        failure_threshold=5,
        timeout_seconds=10
    )


    def __post_init__(self):
        self.config.validate()

class SequencerSystem(Chart):
    def __init__(
        self,
        scope: Construct,
        name: str,
        namespace: str,
        system_structure: Dict[str, Dict[str, Any]],
    ):
        super().__init__(
            scope, name, disable_resource_name_hashes=True, namespace=namespace
        )
        self.mempool = Service(
            self,
            "mempool",
            image="paulbouwer/hello-kubernetes:1.7",
            replicas=2,
            config=system_structure.config.get(),
            health_check=HealthCheck(
                startup_probe=system_structure.startup_probe,
                readiness_probe=system_structure.readiness_probe,
                liveness_probe=system_structure.liveness_probe
            )
        )
        self.batcher = Service(self, "batcher", image="ghost", container_port=2368, health_check=HealthCheck(
                startup_probe=system_structure.startup_probe,
                readiness_probe=system_structure.readiness_probe,
                liveness_probe=system_structure.liveness_probe
            ))
        self.sequencer_node = Service(
            self, 
            "sequencer-node", 
            image="", 
            container_port=8082,
            health_check=HealthCheck(
                startup_probe=system_structure.startup_probe,
                readiness_probe=system_structure.readiness_probe,
                liveness_probe=system_structure.liveness_probe
            )
        )


app = App()
a = SequencerSystem(
    scope=app,
    name="sequencer-system",
    namespace="test-namespace",
    system_structure=SystemStructure(),
)

app.synth()
