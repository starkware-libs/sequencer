#!/usr/bin/env python3

import dataclasses

from constructs import Construct
from cdk8s import App, Chart, YamlOutputType
from typing import Optional

from config.sequencer import Config
from app.service import ServiceApp
from services.topology_helpers import get_dev_config
from services import topology, helpers


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
        self, scope: Construct, name: str, namespace: str, service_topology: topology.ServiceTopology
    ):
        super().__init__(scope, name, disable_resource_name_hashes=True, namespace=namespace)
        self.service = ServiceApp(self, name, namespace=namespace, service_topology=service_topology)


def main():
    args = helpers.argument_parser()
    app = App(yaml_output_type=YamlOutputType.FOLDER_PER_CHART_FILE_PER_RESOURCE)
    system_presets = [
        topology.SequencerDev(
            config=get_dev_config(config)
        ) for config in args.config_file
    ]
    for index, system_preset in enumerate(system_presets):
        SequencerNode(
            scope=app,
            name=f"sequencer-node-{index}",
            namespace=args.namespace,
            service_topology=system_preset,
            )

    app.synth()


if __name__ == "__main__":
    main()
