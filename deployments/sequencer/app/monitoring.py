import json
from typing import Any, Dict, Optional

from constructs import Construct
from cdk8s import Names, ApiObjectMetadata
from imports.co.starkware.grafana.dashboards import (
    SharedGrafanaDashboard,
    SharedGrafanaDashboardSpec,
)
from imports.co.starkware.grafana.alerts import (
    SharedGrafanaAlertRuleGroup,
    SharedGrafanaAlertRuleGroupSpec,
    SharedGrafanaAlertRuleGroupSpecInstanceSelector,
    SharedGrafanaAlertRuleGroupSpecRules,
    SharedGrafanaAlertRuleGroupSpecRulesData,
    SharedGrafanaAlertRuleGroupSpecRulesExecErrState,
    SharedGrafanaAlertRuleGroupSpecRulesNoDataState,
    SharedGrafanaAlertRuleGroupSpecRulesDataRelativeTimeRange
)

from services.monitoring import GrafanaDashboard, GrafanaAlertGroup


class MonitoringApp(Construct):
    def __init__(
        self,
        scope: Construct,
        id: str,
        cluster: str,
        namespace: str,
        grafana_dashboard: GrafanaDashboard,
        grafana_alert_rule_group: GrafanaAlertGroup,
    ) -> None:
        super().__init__(scope, id)

        self.namespace = namespace
        self.cluster = cluster
        self.labels = {
            "app": "sequencer-node",
            "service": Names.to_label_value(self, include_hash=False),
        }
        self.grafana_dashboard = grafana_dashboard.load_dashboard()["dashboard"]
        self.grafana_dashboard["title"] = f"{self.namespace}/Sequencer-Dashboard"
        self.grafana_alert_group = grafana_alert_rule_group
        self.grafana_alert_files = self.grafana_alert_group.get_alert_files()

        grafana_dashboard = self._get_shared_grafana_dashboard()
        grafana_alert_rule_group = self._get_shared_grafana_alert_rule_group()

    def _get_api_object_metadata(self):
        return ApiObjectMetadata(
            labels=self.labels,
        )

    def _get_shared_grafana_dashboard_spec(self):
        return SharedGrafanaDashboardSpec(
            collection_name="shared-grafana-dashboard",
            dashboard_name=self.node.id,
            folder_name=self.cluster,
            dashboard_json=json.dumps(self.grafana_dashboard, indent=4),
        )

    def _get_shared_grafana_dashboard(self):
        return SharedGrafanaDashboard(
            self,
            "shared-grafana-dashboard",
            metadata=self._get_api_object_metadata(),
            spec=self._get_shared_grafana_dashboard_spec(),
        )

    def _get_shared_grafana_alert_rule_group_rules(self, rule: Dict[str, Any]):
        return SharedGrafanaAlertRuleGroupSpecRules(
            uid=rule["name"],
            title=rule["title"],
            condition=rule["condition"],
            for_=rule["for"],
            annotations=rule["annotations"],
            is_paused=rule["isPaused"],
            labels=rule["labels"],
            notification_settings=None,
            exec_err_state=SharedGrafanaAlertRuleGroupSpecRulesExecErrState.ERROR,
            no_data_state=SharedGrafanaAlertRuleGroupSpecRulesNoDataState.NO_DATA,
            data=[
                SharedGrafanaAlertRuleGroupSpecRulesData(
                    datasource_uid=data["datasourceUid"],
                    model=data["model"],
                    query_type=data["queryType"],
                    ref_id=data["refId"],
                    relative_time_range=SharedGrafanaAlertRuleGroupSpecRulesDataRelativeTimeRange(
                        from_=data["relativeTimeRange"]["from"],
                        to=data["relativeTimeRange"]["to"],
                    ),
                )
                for data in rule["data"]
            ],
        )

    def _get_shared_grafana_alert_rule_group_spec(self):
        rules = []
        for alert_file in self.grafana_alert_files:
            alert_rule = self.grafana_alert_group.load_alert(alert_file)
            rules.append(self._get_shared_grafana_alert_rule_group_rules(alert_rule))

        return SharedGrafanaAlertRuleGroupSpec(
            name="shared-grafana-alert-rule-group",
            instance_selector=SharedGrafanaAlertRuleGroupSpecInstanceSelector(),
            interval="10m",
            editable=False,
            folder_uid=self.cluster,
            folder_ref=self.cluster,
            rules=rules,
        )

    def _get_shared_grafana_alert_rule_group(self):
        return SharedGrafanaAlertRuleGroup(
            self,
            "shared-grafana-alert-rule-group",
            metadata=self._get_api_object_metadata(),
            spec=self._get_shared_grafana_alert_rule_group_spec(),
        )
