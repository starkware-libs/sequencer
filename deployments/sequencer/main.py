#!/usr/bin/env python3

from constructs import Construct
from cdk8s import App, Chart
from typing import Dict, Any
from services.service import Service
import dataclasses

from config.sequencer import Config, SequencerDevConfig


@dataclasses.dataclass
class SystemStructure:
    topology: str = "mesh"
    replicas: str = "2"
    size: str = "small"
    config: Config = SequencerDevConfig()

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
        )
        self.batcher = Service(self, "batcher", image="ghost", container_port=2368)
        self.sequencer_node = Service(
            self, 
            "sequencer", 
            image="", 
            container_port=8082, 
            startup_probe_path="/monitoring/nodeVersion", 
            readiness_probe_path="/monitoring/ready", 
            liveness_probe_path="/monitoring/alive"
        )


app = App()
a = SequencerSystem(
    scope=app,
    name="sequencer-system",
    namespace="test-namespace",
    system_structure=SystemStructure(),
)

app.synth()
