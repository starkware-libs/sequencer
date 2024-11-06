#!/usr/bin/env python3

import dataclasses

from constructs import Construct # type: ignore
from cdk8s import App, Chart # type: ignore
from typing import Dict, Any, Optional

from services.service import Service
from config.sequencer import Config, SequencerDevConfig
from services.objects import HealthCheck, ServiceType, Probe
from services import defaults


@dataclasses.dataclass
class SystemStructure:
    topology: str = "mesh"
    replicas: str = "2"
    size: str = "small"
    config: Config = SequencerDevConfig()
    health_check: Optional[HealthCheck] = None

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
            health_check=defaults.health_check
        )
        self.batcher = Service(
            self, 
            "batcher", 
            image="ghost", 
            container_port=2368, 
            health_check=defaults.health_check
        )
        self.sequencer_node = Service(
            self, 
            "sequencer-node", 
            image="",
            container_port=8082,
            replicas=1,
            config=system_structure.config.get(),
            service_type=ServiceType.CLUSTER_IP,
            health_check=HealthCheck(
                startup_probe=Probe(port="http", path="/monitoring/NodeVersion", period_seconds=10, failure_threshold=10, timeout_seconds=5),
                readiness_probe=Probe(port="http", path="/monitoring/ready", period_seconds=10, failure_threshold=5, timeout_seconds=5),
                liveness_probe=Probe(port="http", path="/monitoring/alive", period_seconds=10, failure_threshold=5, timeout_seconds=5)
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
