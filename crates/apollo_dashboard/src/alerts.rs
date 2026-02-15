use std::collections::HashSet;

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

use crate::alert_placeholders::{
    ComparisonValueOrPlaceholder,
    ExpressionOrExpressionWithPlaceholder,
    SeverityValueOrPlaceholder,
};

pub(crate) const PENDING_DURATION_DEFAULT: &str = "30s";
pub(crate) const EVALUATION_INTERVAL_SEC_DEFAULT: u64 = 30;
pub(crate) const SECS_IN_MIN: u64 = 60;

/// Alerts to be configured in the dashboard.
#[derive(Debug, Serialize)]
pub struct Alerts {
    alerts: Vec<Alert>,
}

impl Alerts {
    pub(crate) fn new(alerts: Vec<Alert>) -> Self {
        // Validate that there are no duplicate alert names.
        alerts
            .iter()
            .map(|alert| alert.name.as_str())
            .try_fold(HashSet::new(), |mut set, name| set.insert(name).then_some(set).ok_or(name))
            .unwrap_or_else(|duplicate| panic!("Duplicate alert name found: {duplicate}"));

        // Validate that there are no duplicate placeholder names across all alerts.
        alerts
            .iter()
            .flat_map(|alert| alert.get_placeholder_names().iter())
            .try_fold(HashSet::new(), |mut set, name| {
                set.insert(name.clone()).then_some(set).ok_or(name)
            })
            .unwrap_or_else(|duplicate| {
                panic!("Duplicate placeholder name found across alerts: {duplicate}")
            });

        Self { alerts }
    }
}

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
pub(crate) enum AlertComparisonOp {
    #[serde(rename = "gt")]
    GreaterThan,
    #[serde(rename = "lt")]
    LessThan,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AlertLogicalOp {
    And,
    // TODO(Tsabary): remove the `allow(dead_code)` once this variant is used.
    #[allow(dead_code)]
    Or,
}

/// Defines the condition to trigger the alert.
#[derive(Debug)]
pub(crate) struct AlertCondition {
    // The comparison operator to use when comparing the expression to the value.
    comparison_op: AlertComparisonOp,
    // The value to compare the expression to.
    comparison_value: ComparisonValueOrPlaceholder,
    // The logical operator between this condition and other conditions.
    // TODO(Yael): Consider moving this field to the be one per alert to avoid ambiguity when
    // trying to use a combination of `and` and `or` operators.
    logical_op: AlertLogicalOp,
}

impl AlertCondition {
    pub(crate) fn new(
        comparison_op: AlertComparisonOp,
        comparison_value: impl Into<ComparisonValueOrPlaceholder>,
        logical_op: AlertLogicalOp,
    ) -> Self {
        Self { comparison_op, comparison_value: comparison_value.into(), logical_op }
    }

    pub(crate) fn get_comparison_value_placeholder_name(&self) -> Option<String> {
        match &self.comparison_value {
            ComparisonValueOrPlaceholder::Placeholder(_) => {
                self.comparison_value.unique_alert_placeholder_name()
            }
            ComparisonValueOrPlaceholder::ConcreteValue(_) => None,
        }
    }
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

#[derive(Debug, Serialize)]
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
#[derive(Debug, Serialize)]
pub(crate) struct Alert {
    // The name of the alert.
    name: String,
    // The title that will be displayed.
    title: String,
    // The group that the alert will be displayed under.
    #[serde(rename = "ruleGroup")]
    alert_group: AlertGroup,
    // The expression to evaluate for the alert.
    expr: ExpressionOrExpressionWithPlaceholder,
    // The conditions that must be met for the alert to be triggered.
    conditions: Vec<AlertCondition>,
    // The time duration for which the alert conditions must be true before an alert is triggered.
    #[serde(rename = "for")]
    pending_duration: String,
    // The interval in sec between evaluations of the alert.
    #[serde(rename = "intervalSec")]
    evaluation_interval_sec: u64,
    // The severity level of the alert.
    severity: SeverityValueOrPlaceholder,
    // Indicates if relevant for observer nodes.
    observer_applicable: ObserverApplicability,
    #[serde(skip)]
    placeholder_names: HashSet<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ObserverApplicability {
    Applicable,
    NotApplicable,
}

impl Serialize for ObserverApplicability {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            ObserverApplicability::Applicable => serializer.serialize_str("true"),
            ObserverApplicability::NotApplicable => serializer.serialize_str("false"),
        }
    }
}

impl Alert {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        name: impl ToString,
        title: impl ToString,
        alert_group: AlertGroup,
        expr: impl Into<ExpressionOrExpressionWithPlaceholder>,
        conditions: Vec<AlertCondition>,
        pending_duration: impl ToString,
        evaluation_interval_sec: u64,
        severity: impl Into<SeverityValueOrPlaceholder>,
        observer_applicable: ObserverApplicability,
    ) -> Self {
        let severity = severity.into();

        // Collect all placeholder names from the conditions and severity field.
        let severity_placeholder = severity.unique_alert_placeholder_name();

        // Extract the expression and the placeholder names from the expression field.
        let expr = expr.into();
        let expr_placeholder_names = expr.unique_alert_placeholder_name();

        // Validate there are no duplicate placeholder names.
        let placeholder_names = conditions
            .iter()
            .filter_map(|condition| condition.get_comparison_value_placeholder_name())
            .chain(severity_placeholder)
            .chain(expr_placeholder_names.into_iter().flatten())
            .try_fold(HashSet::new(), |mut set, name| {
                set.insert(name.clone()).then_some(set).ok_or(name)
            })
            .unwrap_or_else(|duplicate| panic!("Duplicate placeholder name found: {duplicate}"));

        Self {
            name: name.to_string(),
            title: title.to_string(),
            alert_group,
            expr,
            conditions,
            pending_duration: pending_duration.to_string(),
            evaluation_interval_sec,
            severity,
            observer_applicable,
            placeholder_names,
        }
    }

    pub(crate) fn get_placeholder_names(&self) -> &HashSet<String> {
        &self.placeholder_names
    }
}
