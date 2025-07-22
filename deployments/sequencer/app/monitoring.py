import json
from typing import Any, Dict, Optional

from cdk8s import ApiObjectMetadata, Names
from constructs import Construct
from imports.alerts.co.starkware.grafana import (
    SharedGrafanaAlertRuleGroup,
    SharedGrafanaAlertRuleGroupSpec,
    SharedGrafanaAlertRuleGroupSpecInstanceSelector,
    SharedGrafanaAlertRuleGroupSpecRules,
    SharedGrafanaAlertRuleGroupSpecRulesData,
    SharedGrafanaAlertRuleGroupSpecRulesDataRelativeTimeRange,
    SharedGrafanaAlertRuleGroupSpecRulesExecErrState,
    SharedGrafanaAlertRuleGroupSpecRulesNoDataState,
)
from imports.dashboards.co.starkware.grafana import (
    SharedGrafanaDashboard,
    SharedGrafanaDashboardSpec,
)
from sequencer.services.config import GrafanaAlertRuleGroupConfig, GrafanaDashboardConfig
from sequencer.services.helpers import generate_random_hash, sanitize_name


class MonitoringApp(Construct):
    def __init__(
        self,
        scope: Construct,
        id: str,
        cluster: str,
        namespace: str,
    ) -> None:
        super().__init__(scope, id)

        self.namespace = namespace
        self.cluster = cluster

    def _get_api_object_metadata(self) -> ApiObjectMetadata:
        return ApiObjectMetadata(
            labels={
                "app": "sequencer-node",
                "service": "monitoring",
            },
        )


class GrafanaDashboardApp(MonitoringApp):
    def __init__(
        self,
        scope: Construct,
        id: str,
        cluster: str,
        namespace: str,
        grafana_dashboard: GrafanaDashboardConfig,
    ) -> None:
        super().__init__(scope, id, cluster, namespace)

        self.grafana_dashboard = grafana_dashboard.load()["dashboard"]
        self.grafana_dashboard["title"] = f"sequencer-{self.namespace}-dashboard"
        grafana_dashboard = self._get_shared_grafana_dashboard()

    def _get_shared_grafana_dashboard_spec(self) -> SharedGrafanaDashboardSpec:
        return SharedGrafanaDashboardSpec(
            collection_name="shared-grafana-dashboard",
            dashboard_name=Names.to_dns_label(self, include_hash=False),
            folder_name=self.cluster,
            dashboard_json=json.dumps(self.grafana_dashboard, indent=4),
        )

    def _get_shared_grafana_dashboard(self) -> SharedGrafanaDashboard:
        return SharedGrafanaDashboard(
            self,
            self.node.id,
            metadata=self._get_api_object_metadata(),
            spec=self._get_shared_grafana_dashboard_spec(),
        )


class GrafanaAlertRuleGroupApp(MonitoringApp):
    def __init__(
        self,
        scope: Construct,
        id: str,
        cluster: str,
        namespace: str,
        grafana_alert_rule_group: GrafanaAlertRuleGroupConfig,
    ) -> None:
        super().__init__(scope, id, cluster, namespace)

        self.grafana_alert_group = grafana_alert_rule_group
        self.grafana_alert_files = self.grafana_alert_group.get_alert_files()
        grafana_alert_rule_group = self._get_shared_grafana_alert_rule_group()

    def _exec_err_state_enum_selector(
        self, exec_err_state: str
    ) -> Optional[SharedGrafanaAlertRuleGroupSpecRulesExecErrState]:
        if exec_err_state.upper() == "OK":
            return SharedGrafanaAlertRuleGroupSpecRulesExecErrState.OK
        elif exec_err_state.upper() == "ERROR":
            return SharedGrafanaAlertRuleGroupSpecRulesExecErrState.ERROR
        elif exec_err_state.upper() == "ALERTING":
            return SharedGrafanaAlertRuleGroupSpecRulesExecErrState.ALERTING
        elif exec_err_state.upper() == "KEEPLAST":
            return SharedGrafanaAlertRuleGroupSpecRulesExecErrState.KEEP_LAST
        else:
            return None

    def _exec_no_data_state_enum_selector(
        self, no_data_state: str
    ) -> Optional[SharedGrafanaAlertRuleGroupSpecRulesNoDataState]:
        if no_data_state.upper() == "OK":
            return SharedGrafanaAlertRuleGroupSpecRulesNoDataState.OK
        elif no_data_state.upper() == "NODATA":
            return SharedGrafanaAlertRuleGroupSpecRulesNoDataState.NO_DATA
        elif no_data_state.upper() == "ALERTING":
            return SharedGrafanaAlertRuleGroupSpecRulesNoDataState.ALERTING
        elif no_data_state.upper() == "KEEPLAST":
            return SharedGrafanaAlertRuleGroupSpecRulesNoDataState.KEEP_LAST
        else:
            return None

    def _get_shared_grafana_alert_rule_group_rules(
        self, rule: Dict[str, Any]
    ) -> SharedGrafanaAlertRuleGroupSpecRules:
        title = f'{self.namespace}-{rule["title"].replace(" ", "_").lower()}'
        uid = generate_random_hash(length=30, from_string=title)

        return SharedGrafanaAlertRuleGroupSpecRules(
            uid=uid,
            title=title,
            condition=rule["condition"],
            for_=rule["for"],
            annotations=rule["annotations"],
            is_paused=rule["isPaused"],
            labels=rule["labels"],
            notification_settings=None,
            exec_err_state=self._exec_err_state_enum_selector(rule["execErrState"]),
            no_data_state=self._exec_no_data_state_enum_selector(rule["noDataState"]),
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

    def _get_shared_grafana_alert_rule_group_spec(self) -> SharedGrafanaAlertRuleGroupSpec:
        rules = []
        for alert_file in self.grafana_alert_files:
            alert_rule = self.grafana_alert_group.load(alert_file)
            rules.append(self._get_shared_grafana_alert_rule_group_rules(alert_rule))

        return SharedGrafanaAlertRuleGroupSpec(
            name=sanitize_name(f"{self.cluster}-{self.namespace}-{self.node.id}"),
            instance_selector=SharedGrafanaAlertRuleGroupSpecInstanceSelector(),
            interval="1m",
            editable=False,
            folder_ref=self.cluster,
            rules=rules,
        )

    def _get_shared_grafana_alert_rule_group(self) -> SharedGrafanaAlertRuleGroup:
        return SharedGrafanaAlertRuleGroup(
            self,
            self.node.id,
            metadata=self._get_api_object_metadata(),
            spec=self._get_shared_grafana_alert_rule_group_spec(),
        )
