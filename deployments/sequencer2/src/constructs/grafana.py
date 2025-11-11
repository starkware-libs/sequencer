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
from src.config.loaders import GrafanaAlertRuleGroupConfigLoader, GrafanaDashboardConfigLoader
from src.utils import generate_random_hash, sanitize_name


class GrafanaBaseConstruct(Construct):
    """Base construct for Grafana resources with shared functionality."""

    def __init__(
        self,
        scope: Construct,
        id: str,
        cluster: str,
        namespace: str,
    ) -> None:
        super().__init__(scope, id)
        self.cluster = cluster
        self.namespace = namespace

    def _get_api_object_metadata(self, name: Optional[str] = None):
        """Common metadata for Grafana resources."""
        metadata_kwargs = {
            "labels": {
                "app": "sequencer",
                "service": "monitoring",
            },
        }
        if name:
            metadata_kwargs["name"] = name
        return ApiObjectMetadata(**metadata_kwargs)


class GrafanaDashboardConstruct(GrafanaBaseConstruct):
    """Construct for creating Grafana Dashboard resources."""

    def __init__(
        self,
        scope: Construct,
        id: str,
        cluster: str,
        namespace: str,
        grafana_dashboard: GrafanaDashboardConfigLoader,
    ) -> None:
        super().__init__(scope, id, cluster, namespace)

        self.grafana_dashboard = grafana_dashboard.load()["dashboard"]
        self.grafana_dashboard["title"] = f"sequencer-{self.namespace}-dashboard"
        self.hash_value = generate_random_hash(from_string=f"{self.cluster}-{self.namespace}")
        self.custom_name = f"{self.namespace}-dash-{self.hash_value}"
        self._get_shared_grafana_dashboard()

    def _get_shared_grafana_dashboard_spec(self):
        return SharedGrafanaDashboardSpec(
            collection_name=f"shared-grafana-dashboard",
            dashboard_name=self.custom_name,
            folder_name=self.cluster,
            dashboard_json=json.dumps(self.grafana_dashboard, indent=4),
        )

    def _get_shared_grafana_dashboard(self):
        return SharedGrafanaDashboard(
            self,
            self.node.id,
            metadata=self._get_api_object_metadata(name=self.custom_name),
            spec=self._get_shared_grafana_dashboard_spec(),
        )


class GrafanaAlertRuleGroupConstruct(GrafanaBaseConstruct):
    """Construct for creating Grafana Alert Rule Group resources."""

    def __init__(
        self,
        scope: Construct,
        id: str,
        cluster: str,
        namespace: str,
        grafana_alert_rule_group: GrafanaAlertRuleGroupConfigLoader,
    ) -> None:
        super().__init__(scope, id, cluster, namespace)

        self.grafana_alert_group = grafana_alert_rule_group
        self.grafana_alert_files = self.grafana_alert_group.get_alert_files()
        self.hash_value = generate_random_hash(from_string=f"{self.cluster}-{self.namespace}")
        self.custom_name = f"{self.namespace}-arg-{self.hash_value}"
        self._get_shared_grafana_alert_rule_group()

    def _exec_err_state_enum_selector(self, exec_err_state: str) -> Optional[str]:
        """Convert string to ExecErrState enum."""
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

    def _exec_no_data_state_enum_selector(self, no_data_state: str) -> Optional[str]:
        """Convert string to NoDataState enum."""
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

    def _get_shared_grafana_alert_rule_group_rules(self, rule: Dict[str, Any]):
        """Convert alert rule dict to SharedGrafanaAlertRuleGroupSpecRules."""
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

    def _get_shared_grafana_alert_rule_group_spec(self):
        """Build the spec for the alert rule group."""
        rules = []
        for alert_file in self.grafana_alert_files:
            alert_rule = self.grafana_alert_group.load(str(alert_file))
            rules.append(self._get_shared_grafana_alert_rule_group_rules(alert_rule))

        return SharedGrafanaAlertRuleGroupSpec(
            name=self.custom_name,
            instance_selector=SharedGrafanaAlertRuleGroupSpecInstanceSelector(),
            interval="1m",
            editable=False,
            folder_ref=self.cluster,
            rules=rules,
        )

    def _get_shared_grafana_alert_rule_group(self):
        """Create the SharedGrafanaAlertRuleGroup resource."""
        return SharedGrafanaAlertRuleGroup(
            self,
            self.node.id,
            metadata=self._get_api_object_metadata(name=self.custom_name),
            spec=self._get_shared_grafana_alert_rule_group_spec(),
        )
