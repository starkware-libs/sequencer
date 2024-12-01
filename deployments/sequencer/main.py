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
        self, scope: Construct, name: str, namespace: str, topology: topology.ServiceTopology
    ):
        super().__init__(scope, name, disable_resource_name_hashes=True, namespace=namespace)
        self.service = ServiceApp(self, name, namespace=namespace, topology=topology)


def main():
    args = helpers.argument_parser()
    if args.env == "dev":
        system_preset = topology.SequencerDev(config=get_dev_config(args.config_file))
    elif args.env == "prod":
        raise NotImplementedError("Production environment not supported.")
        # system_preset = topology.SequencerProd()

    app = App(yaml_output_type=YamlOutputType.FOLDER_PER_CHART_FILE_PER_RESOURCE)

    SequencerNode(
        scope=app,
        name="sequencer-node",
        namespace=args.namespace,
        topology=system_preset,
    )

    app.synth()


if __name__ == "__main__":
    main()
