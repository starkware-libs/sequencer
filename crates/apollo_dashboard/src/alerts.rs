use std::collections::HashSet;
use std::fmt;

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use strum_macros::EnumIter;

/// Alerts to be configured in the dashboard.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Alerts {
    alerts: Vec<Alert>,
}

impl Alerts {
    pub(crate) fn new(alerts: Vec<Alert>, alert_env_filtering: AlertEnvFiltering) -> Self {
        let alerts: Vec<Alert> = alerts
            .into_iter()
            .filter(|alert| alert.alert_env_filtering.matches(&alert_env_filtering))
            .collect();
        let mut alert_names: HashSet<&str> = HashSet::new();

        for alert in &alerts {
            if !alert_names.insert(alert.name.as_str()) {
                panic!(
                    "Duplicate alert name found: {} for env: {}",
                    alert.name, alert.alert_env_filtering
                );
            }
        }
        Self { alerts }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumIter)]
pub enum AlertEnvFiltering {
    All,
    MainnetStyleAlerts,
    TestnetStyleAlerts,
}

impl AlertEnvFiltering {
    pub fn matches(&self, target: &AlertEnvFiltering) -> bool {
        self == target || *self == AlertEnvFiltering::All
    }
}

impl fmt::Display for AlertEnvFiltering {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            AlertEnvFiltering::All => {
                unreachable!()
            } // This variant is used for internal logic and should not be displayed.
            AlertEnvFiltering::MainnetStyleAlerts => "mainnet",
            AlertEnvFiltering::TestnetStyleAlerts => "testnet",
        };
        write!(f, "{}", s)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) enum AlertSeverity {
    /// Critical issues that demand immediate attention. These are high-impact incidents that
    /// affect the system's availability.
    #[serde(rename = "p1")]
    Sos,
    /// Standard alerts for production issues that require attention around the clock but are not
    /// as time-sensitive as SOS alerts.
    #[serde(rename = "p2")]
    Regular,
    /// Important alerts that do not require overnight attention. These are delayed during night
    /// hours to reduce unnecessary off-hours noise.
    #[serde(rename = "p3")]
    DayOnly,
    /// Alerts that are only triggered during official business hours. These do not trigger during
    /// holidays.
    #[serde(rename = "p4")]
    WorkingHours,
    /// Non-critical alerts, meant purely for information. These are not intended to wake anyone up
    /// and are monitored only by the development team.
    #[serde(rename = "p5")]
    Informational,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) enum AlertComparisonOp {
    #[serde(rename = "gt")]
    GreaterThan,
    #[serde(rename = "lt")]
    LessThan,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AlertLogicalOp {
    And,
    // TODO(Tsabary): remove the `allow(dead_code)` once this variant is used.
    #[allow(dead_code)]
    Or,
}

/// Defines the condition to trigger the alert.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AlertCondition {
    // The comparison operator to use when comparing the expression to the value.
    pub(crate) comparison_op: AlertComparisonOp,
    // The value to compare the expression to.
    pub(crate) comparison_value: f64,
    // The logical operator between this condition and other conditions.
    // TODO(Yael): Consider moving this field to the be one per alert to avoid ambiguity when
    // trying to use a combination of `and` and `or` operators.
    pub(crate) logical_op: AlertLogicalOp,
}

impl Serialize for AlertCondition {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("AlertCondition", 4)?;

        state.serialize_field(
            "evaluator",
            &serde_json::json!({
                "params": [self.comparison_value],
                "type": self.comparison_op
            }),
        )?;

        state.serialize_field(
            "operator",
            &serde_json::json!({
                "type": self.logical_op
            }),
        )?;

        state.serialize_field(
            "reducer",
            &serde_json::json!({
                "params": [],
                "type": "avg"
            }),
        )?;

        state.serialize_field("type", "query")?;

        state.end()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AlertGroup {
    Batcher,
    Consensus,
    Gateway,
    General,
    HttpServer,
    L1GasPrice,
    L1Messages,
    Mempool,
    StateSync,
}

/// Describes the properties of an alert defined in grafana.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct Alert {
    // The name of the alert.
    name: String,
    // The title that will be displayed.
    title: String,
    // The group that the alert will be displayed under.
    #[serde(rename = "ruleGroup")]
    alert_group: AlertGroup,
    // The expression to evaluate for the alert.
    expr: String,
    // The conditions that must be met for the alert to be triggered.
    conditions: Vec<AlertCondition>,
    // The time duration for which the alert conditions must be true before an alert is triggered.
    #[serde(rename = "for")]
    pending_duration: String,
    // The interval in sec between evaluations of the alert.
    #[serde(rename = "intervalSec")]
    evaluation_interval_sec: u64,
    // The severity level of the alert.
    severity: AlertSeverity,
    #[serde(skip)]
    alert_env_filtering: AlertEnvFiltering,
}

impl Alert {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        name: impl ToString,
        title: impl ToString,
        alert_group: AlertGroup,
        expr: impl ToString,
        conditions: Vec<AlertCondition>,
        pending_duration: impl ToString,
        evaluation_interval_sec: u64,
        severity: AlertSeverity,
        alert_env_filtering: AlertEnvFiltering,
    ) -> Self {
        Self {
            name: name.to_string(),
            title: title.to_string(),
            alert_group,
            expr: expr.to_string(),
            conditions,
            pending_duration: pending_duration.to_string(),
            evaluation_interval_sec,
            severity,
            alert_env_filtering,
        }
    }
}
