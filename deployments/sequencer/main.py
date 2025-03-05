#!/usr/bin/env python3

import dataclasses

from constructs import Construct
from cdk8s import App, Chart, YamlOutputType
from typing import Optional

from config.sequencer import Config
from app.service import ServiceApp
from services import topology, helpers
from config.sequencer import SequencerDevConfig


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
        service_topology: topology.ServiceTopology,
    ):
        super().__init__(scope, name, disable_resource_name_hashes=True, namespace=namespace)
        self.service = ServiceApp(self, name, namespace=namespace, service_topology=service_topology)


def main():
    args = helpers.argument_parser()
    app = App(yaml_output_type=YamlOutputType.FOLDER_PER_CHART_FILE_PER_RESOURCE)

    preset = topology.DeploymentConfig(args.deployment_config_file)
    services = preset.get_services()

    for svc in services:
        SequencerNode(
            scope=app,
            name=svc["name"].lower(),
            namespace=args.namespace,
            service_topology=topology.ServiceTopology(
                config=SequencerDevConfig(config_file_path=svc["config_path"]),
                image=preset.get_image(),
                autoscale=svc["autoscale"],
                ingress=svc["ingress"],
                storage=svc["storage"],
            ),
        )

    app.synth()


if __name__ == "__main__":
    main()
