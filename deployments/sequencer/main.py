#!/usr/bin/env python3


from constructs import Construct
from cdk8s import App, Chart, YamlOutputType

from app.service import ServiceApp
from services import topology, helpers, config


class SequencerNode(Chart):
    def __init__(
        self,
        scope: Construct,
        name: str,
        namespace: str,
        service_topology: topology.ServiceTopology,
    ):
        super().__init__(scope, name, disable_resource_name_hashes=True, namespace=namespace)
        self.service = ServiceApp(
            self, name, namespace=namespace, service_topology=service_topology
        )


def main():
    args = helpers.argument_parser()
    app = App(yaml_output_type=YamlOutputType.FOLDER_PER_CHART_FILE_PER_RESOURCE)

    preset = config.DeploymentConfig(args.deployment_config_file)
    services = preset.get_services()
    image = preset.get_image()
    application_config_subdir = preset.get_application_config_subdir()

    for svc in services:
        SequencerNode(
            scope=app,
            name=f'sequencer-{svc["name"].lower()}',
            namespace=args.namespace,
            service_topology=topology.ServiceTopology(
                config=config.SequencerConfig(
                    config_subdir=application_config_subdir, config_path=svc["config_path"]
                ),
                image=image,
                replicas=svc["replicas"],
                autoscale=svc["autoscale"],
                ingress=svc["ingress"],
                storage=svc["storage"],
            ),
        )

    app.synth()


if __name__ == "__main__":
    main()
