import json

from constructs import Construct
from cdk8s import Names, ApiObjectMetadata
from imports.co.starkware.grafana import SharedGrafanaDashboard, SharedGrafanaDashboardSpec

from services.monitoring import GrafanaDashboard


class MonitoringApp(Construct):
    def __init__(
        self, scope: Construct, id: str, namespace: str, grafana_dashboard: GrafanaDashboard
    ) -> None:
        super().__init__(scope, id)

        self.namespace = namespace
        self.labels = {
            "app": "sequencer-node",
            "service": Names.to_label_value(self, include_hash=False),
        }

        SharedGrafanaDashboard(
            self,
            "shared-grafana-dashboard",
            metadata=ApiObjectMetadata(
                labels=self.labels,
            ),
            spec=SharedGrafanaDashboardSpec(
                collection_name="shared-grafana-dashboard",
                dashboard_name=self.node.id,
                folder_name=self.namespace,
                dashboard_json=json.dumps(grafana_dashboard.get(), indent=2),
            ),
        )
