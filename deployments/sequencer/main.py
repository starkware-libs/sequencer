#!/usr/bin/env python3

import sys

from constructs import Construct
from cdk8s import App, Chart, YamlOutputType

from app.service import ServiceApp
from app.monitoring import MonitoringApp
from services import topology, helpers, config, monitoring


class SequencerNode(Chart):
    def __init__(
        self,
        scope: Construct,
        name: str,
        namespace: str,
        monitoring: bool,
        service_topology: topology.ServiceTopology,
    ):
        super().__init__(scope, name, disable_resource_name_hashes=True, namespace=namespace)
        self.service = ServiceApp(
            self,
            name,
            namespace=namespace,
            service_topology=service_topology,
            monitoring=monitoring,
        )


class SequencerMonitoring(Chart):
    def __init__(
        self,
        scope: Construct,
        name: str,
        cluster: str,
        namespace: str,
        grafana_dashboard: monitoring.GrafanaDashboard,
    ):
        super().__init__(scope, name, disable_resource_name_hashes=True, namespace=namespace)
        self.dashboard = MonitoringApp(
            self, name, cluster=cluster, namespace=namespace, grafana_dashboard=grafana_dashboard
        )


def main():
    args = helpers.argument_parser()

    assert not (
        args.create_monitoring and not args.cluster
    ), "Error: --cluster is required when --create-monitoring is provided."

    app = App(yaml_output_type=YamlOutputType.FOLDER_PER_CHART_FILE_PER_RESOURCE)

    preset = config.DeploymentConfig(args.deployment_config_file)
    services = preset.get_services()
    image = preset.get_image()
    application_config_subdir = preset.get_application_config_subdir()

    for svc in services:
        SequencerNode(
            scope=app,
            name=helpers.sanitize_name(f'sequencer-{svc["name"]}'),
            namespace=helpers.sanitize_name(args.namespace),
            monitoring=args.create_monitoring,
            service_topology=topology.ServiceTopology(
                config=config.SequencerConfig(
                    config_subdir=application_config_subdir, config_paths=svc["config_paths"]
                ),
                image=image,
                controller=svc["controller"].lower(),
                replicas=svc["replicas"],
                autoscale=svc["autoscale"],
                ingress=svc["ingress"],
                storage=svc["storage"],
                toleration=svc["toleration"],
                resources=svc["resources"],
                external_secret=svc["external_secret"],
            ),
        )

    if args.create_monitoring:
        dashboard_hash = helpers.generate_random_hash(from_string=f"{args.cluster}-{args.namespace}")
        SequencerMonitoring(
            scope=app,
            name=helpers.sanitize_name(f"sequencer-monitoring-{dashboard_hash}"),
            cluster=args.cluster,
            namespace=helpers.sanitize_name(args.namespace),
            grafana_dashboard=monitoring.GrafanaDashboard("sequencer_node_dashboard.json"),
        )

    app.synth()


if __name__ == "__main__":
    main()
