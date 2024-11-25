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


env = os.getenv("ENV", "dev")


if env == "dev":
    system_preset = defaults.sequencer_dev
elif env == "prod":
    system_preset = defaults.sequencer_prod 


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
            deployment=system_preset.deployment,
            config=system_preset.config,
            image=system_preset.image,
            args=system_preset.args,
            port_mappings=system_preset.port_mappings,
            service_type=system_preset.service_type,
            replicas=system_preset.replicas,
            health_check=system_preset.health_check,
            pvc=system_preset.pvc,
            ingress=system_preset.ingress
        )


app = App(
    yaml_output_type=YamlOutputType.FOLDER_PER_CHART_FILE_PER_RESOURCE
)

sequencer_node = SequencerNode(
    scope=app,
    name=system_preset.name,
    namespace=system_preset.namespace
)

app.synth()
