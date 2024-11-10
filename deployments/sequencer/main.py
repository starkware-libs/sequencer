#!/usr/bin/env python3

import dataclasses

from constructs import Construct # type: ignore
from cdk8s import App, Chart, YamlOutputType # type: ignore
from typing import Dict, Any, Optional

from services.service import Service
from config.sequencer import Config, SequencerDevConfig
from services.objects import (
    HealthCheck, ServiceType, Probe, PersistentVolumeClaim, PortMappings
)
from services import defaults


@dataclasses.dataclass
class SystemStructure:
    topology: str = "mesh"
    replicas: str = "2"
    size: str = "small"
    config: Optional[Config] = None

    def __post_init__(self):
        self.config.validate()


class SequencerNode(Chart):
    def __init__(
        self,
        scope: Construct,
        name: str,
        namespace: str,
        config: Config,
    ):
        super().__init__(
            scope, name, disable_resource_name_hashes=True, namespace=namespace
        )
        self.service = Service(
            self,
            "sequencer-node",
            image="us.gcr.io/starkware-dev/sequencer-node:0.0.3",
            port_mappings=[
                PortMappings(name="http", port=80, container_port=8080),
                PortMappings(name="rpc", port=8081, container_port=8081),
                PortMappings(name="monitoring", port=8082, container_port=8082)
            ],
            service_type=ServiceType.CLUSTER_IP,
            replicas=1,
            config=config,
            health_check=HealthCheck(
                startup_probe=Probe(port=8082, path="/monitoring/NodeVersion", period_seconds=10, failure_threshold=10, timeout_seconds=5),
                readiness_probe=Probe(port=8082, path="/monitoring/ready", period_seconds=10, failure_threshold=5, timeout_seconds=5),
                liveness_probe=Probe(port=8082, path="/monitoring/alive", period_seconds=10, failure_threshold=5, timeout_seconds=5)
            ),
            pvc=PersistentVolumeClaim(
                access_modes=["ReadWriteOnce"],
                storage_class_name="premium-rwo",
                volume_mode="Filesystem",
                storage="256Gi",
                mount_path="/data",
                read_only=False
            )
        )


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
            config=system_structure.config,
            health_check=defaults.health_check
        )
        self.batcher = Service(
            self, 
            "batcher", 
            image="ghost",
            port_mappings=[{"port": 80, "container_port": 2368}],
            health_check=defaults.health_check
        )


app = App(
    yaml_output_type=YamlOutputType.FOLDER_PER_CHART_FILE_PER_RESOURCE
)
sequencer_node = SequencerNode(
    scope=app,
    name="sequencer-node",
    namespace="sequencer",
    config=SequencerDevConfig(mount_path="/app/config")
)
a = SequencerSystem(
    scope=app,
    name="sequencer-system",
    namespace="test-namespace",
    system_structure=SystemStructure(config=SequencerDevConfig(mount_path="/app/config")),
)

app.synth()
