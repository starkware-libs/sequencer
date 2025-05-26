import json

from cdk8s import ApiObjectMetadata, Names
from constructs import Construct
from imports.co.starkware.grafana import SharedGrafanaDashboard, SharedGrafanaDashboardSpec
from services.monitoring import GrafanaDashboard


class MonitoringApp(Construct):
    def __init__(
        self,
        scope: Construct,
        id: str,
        cluster: str,
        namespace: str,
        grafana_dashboard: GrafanaDashboard,
    ) -> None:
        super().__init__(scope, id)

        self.namespace = namespace
        self.labels = {
            "app": "sequencer-node",
            "service": Names.to_label_value(self, include_hash=False),
        }
        self.grafana_dashboard = grafana_dashboard.get_dashboard()["dashboard"]
        self.grafana_dashboard["title"] = f"{self.namespace}/Sequencer-Dashboard"

        SharedGrafanaDashboard(
            self,
            "shared-grafana-dashboard",
            metadata=ApiObjectMetadata(
                labels=self.labels,
            ),
            spec=SharedGrafanaDashboardSpec(
                collection_name="shared-grafana-dashboard",
                dashboard_name=self.node.id,
                folder_name=cluster,
                dashboard_json=json.dumps(self.grafana_dashboard, indent=4),
            ),
        )
