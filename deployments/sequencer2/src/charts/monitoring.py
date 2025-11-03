from typing import Optional

from cdk8s import Chart
from constructs import Construct

from src.config.loaders import (
    GrafanaAlertRuleGroupConfigLoader,
    GrafanaDashboardConfigLoader,
)
from src.constructs.grafana import (
    GrafanaAlertRuleGroupConstruct,
    GrafanaDashboardConstruct,
)
from src.utils import generate_random_hash, sanitize_name


class MonitoringChart(Chart):
    def __init__(
        self,
        scope: Construct,
        id: str,
        cluster: str,
        namespace: str,
        grafana_dashboard: Optional[GrafanaDashboardConfigLoader],
        grafana_alert_rule_group: Optional[GrafanaAlertRuleGroupConfigLoader],
    ):
        super().__init__(scope, id, disable_resource_name_hashes=True, namespace=namespace)
        self.hash = generate_random_hash(from_string=f"{cluster}-{namespace}")

        if grafana_dashboard:
            self.dashboard = GrafanaDashboardConstruct(
                self,
                sanitize_name(f"dashboard-{self.hash}"),
                cluster=cluster,
                namespace=namespace,
                grafana_dashboard=grafana_dashboard,
            )

        if grafana_alert_rule_group:
            self.alert_rule_group = GrafanaAlertRuleGroupConstruct(
                self,
                sanitize_name(f"alert-rule-group-{self.hash}"),
                cluster=cluster,
                namespace=namespace,
                grafana_alert_rule_group=grafana_alert_rule_group,
            )
