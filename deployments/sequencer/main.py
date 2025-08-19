#!/usr/bin/env python3

from typing import Optional

from app.monitoring import GrafanaAlertRuleGroupApp, GrafanaDashboardApp
from app.service import ServiceApp
from cdk8s import App, Chart, YamlOutputType
from constructs import Construct
from services.config import (
    DeploymentConfig,
    GrafanaAlertRuleGroupConfig,
    GrafanaDashboardConfig,
    SequencerConfig,
)
from services.helpers import argument_parser, generate_random_hash, sanitize_name
from services.topology import ServiceTopology


class SequencerNode(Chart):
    def __init__(
        self,
        scope: Construct,
        name: str,
        namespace: str,
        monitoring: bool,
        service_topology: ServiceTopology,
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
        id: str,
        cluster: str,
        namespace: str,
        grafana_dashboard: Optional[GrafanaDashboardConfig],
        grafana_alert_rule_group: Optional[GrafanaAlertRuleGroupConfig],
    ):
        super().__init__(scope, id, disable_resource_name_hashes=True, namespace=namespace)

        self.hash = generate_random_hash(from_string=f"{cluster}-{namespace}")

        self.dashboard = (
            GrafanaDashboardApp(
                self,
                sanitize_name(f"dashboard-{self.hash}"),
                cluster=cluster,
                namespace=namespace,
                grafana_dashboard=grafana_dashboard,
            )
            if grafana_dashboard
            else None
        )

        self.alert_rule_group = (
            GrafanaAlertRuleGroupApp(
                self,
                sanitize_name(f"alert-rule-group-{self.hash}"),
                cluster=cluster,
                namespace=namespace,
                grafana_alert_rule_group=grafana_alert_rule_group,
            )
            if grafana_alert_rule_group
            else None
        )


def main():
    args = argument_parser()

    assert not (
        args.monitoring_dashboard_file and not args.cluster
    ), "Error: --cluster is required when --monitoring-dashboard-file is provided."

    app = App(yaml_output_type=YamlOutputType.FOLDER_PER_CHART_FILE_PER_RESOURCE)
    preset = DeploymentConfig(args.deployment_config_file)
    services = preset.get_services()

    if args.deployment_image:
        image = args.deployment_image
    else:
        # Set default tag if not provided
        if not args.deployment_image_tag:
            args.deployment_image_tag = "dev"
        image = f"ghcr.io/starkware-libs/sequencer/sequencer:{args.deployment_image_tag}"
    application_config_subdir = preset.get_application_config_subdir()
    create_monitoring = True if args.monitoring_dashboard_file else False

    for svc in services:
        SequencerNode(
            scope=app,
            name=sanitize_name(f'sequencer-{svc["name"]}'),
            namespace=sanitize_name(args.namespace),
            monitoring=create_monitoring,
            service_topology=ServiceTopology(
                config=SequencerConfig(
                    config_subdir=application_config_subdir,
                    config_paths=svc["config_paths"],
                ),
                image=image,
                k8s_service_config=svc["k8s_service_config"],
                controller=svc["controller"].lower(),
                update_strategy_type=svc["update_strategy_type"],
                replicas=svc["replicas"],
                autoscale=svc["autoscale"],
                ingress=svc["ingress"],
                storage=svc["storage"],
                toleration=svc["toleration"],
                anti_affinity=svc["anti_affinity"],
                resources=svc["resources"],
                external_secret=svc["external_secret"],
            ),
        )

    grafana_dashboard_config = (
        GrafanaDashboardConfig(dashboard_file_path=args.monitoring_dashboard_file)
        if args.monitoring_dashboard_file
        else None
    )

    grafana_alert_rule_group_config = (
        GrafanaAlertRuleGroupConfig(alerts_folder_path=args.monitoring_alerts_folder)
        if args.monitoring_alerts_folder
        else None
    )

    SequencerMonitoring(
        scope=app,
        id="sequencer-monitoring",
        cluster=args.cluster,
        namespace=sanitize_name(args.namespace),
        grafana_dashboard=grafana_dashboard_config,
        grafana_alert_rule_group=grafana_alert_rule_group_config,
    )

    app.synth()


if __name__ == "__main__":
    main()
