#!/usr/bin/env python3

import dataclasses
import os

from constructs import Construct # type: ignore
from cdk8s import App, Chart, YamlOutputType # type: ignore
from typing import Dict, Any, Optional

from services.service import Service
from config.sequencer import Config
from services.objects import *
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
        namespace: str
    ):
        super().__init__(
            scope, name, disable_resource_name_hashes=True, namespace=namespace
        )
        self.service = Service(
            self,
            name,
            namespace=namespace,
            deployment=True,
            config=defaults.sequencer.config,
            image=defaults.sequencer.image,
            args=defaults.sequencer.args,
            port_mappings=defaults.sequencer.port_mappings,
            service_type=defaults.sequencer.service_type,
            replicas=defaults.sequencer.replicas,
            health_check=defaults.sequencer.health_check,
            pvc=defaults.sequencer.pvc,
            ingress=defaults.sequencer.ingress
        )


# class SequencerSystem(Chart):
#     def __init__(
#         self,
#         scope: Construct,
#         name: str,
#         namespace: str,
#         system_structure: Dict[str, Dict[str, Any]],
#     ):
#         super().__init__(
#             scope, name, disable_resource_name_hashes=True, namespace=namespace
#         )
#         self.mempool = Service(
#             self,
#             "mempool",
#             image="paulbouwer/hello-kubernetes:1.7",
#             replicas=2,
#             config=system_structure.config,
#             health_check=defaults.health_check
#         )
#         self.batcher = Service(
#             self, 
#             "batcher", 
#             image="ghost",
#             port_mappings=[
#                 PortMapping(name="http", port=80, container_port=2368)
#             ],
#             health_check=defaults.health_check
#         )


app = App(
    yaml_output_type=YamlOutputType.FOLDER_PER_CHART_FILE_PER_RESOURCE
)

sequencer_node = SequencerNode(
    scope=app,
    name=defaults.sequencer.name,
    namespace=defaults.sequencer.namespace
)

# a = SequencerSystem(
#     scope=app,
#     name="sequencer-system",
#     namespace="test-namespace",
#     system_structure=SystemStructure(config=SequencerDevConfig(mount_path="/app/config")),
# )

app.synth()
