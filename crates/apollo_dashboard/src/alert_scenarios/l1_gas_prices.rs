use apollo_l1_gas_price::metrics::{
    ETH_TO_STRK_ERROR_COUNT,
    ETH_TO_STRK_RATE,
    ETH_TO_STRK_SUCCESS_COUNT,
    L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY,
    L1_GAS_PRICE_SCRAPER_SUCCESS_COUNT,
    SNIP35_STRK_USD_ERROR_COUNT,
    SNIP35_STRK_USD_RATE,
    SNIP35_STRK_USD_SUCCESS_COUNT,
};
use apollo_metrics::metrics::MetricQueryName;

use crate::alert_placeholders::SeverityValueOrPlaceholder;
use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertLogicalOp,
    AlertSeverity,
    EvaluationRate,
    ObserverApplicability,
    PENDING_DURATION_DEFAULT,
};
use crate::query_builder::sum_increase;

pub(crate) fn get_eth_to_strk_success_count_alert() -> Alert {
    const ALERT_NAME: &str = "eth_to_strk_success_count";
    oracle_success_count_alert(
        ALERT_NAME,
        "Eth to Strk success count",
        &ETH_TO_STRK_SUCCESS_COUNT,
        SeverityValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
    )
}

pub(crate) fn get_eth_to_strk_error_count_alert() -> Alert {
    oracle_error_count_alert(
        "eth_to_strk_error_count",
        "Eth to Strk error count",
        &ETH_TO_STRK_ERROR_COUNT,
        AlertSeverity::Informational,
    )
}

pub(crate) fn get_strk_to_usd_success_count_alert() -> Alert {
    const ALERT_NAME: &str = "strk_to_usd_success_count";
    oracle_success_count_alert(
        ALERT_NAME,
        "Strk to Usd success count",
        &SNIP35_STRK_USD_SUCCESS_COUNT,
        SeverityValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
    )
}

pub(crate) fn get_strk_to_usd_error_count_alert() -> Alert {
    oracle_error_count_alert(
        "strk_to_usd_error_count",
        "Strk to Usd error count",
        &SNIP35_STRK_USD_ERROR_COUNT,
        AlertSeverity::Informational,
    )
}

pub(crate) fn get_eth_to_strk_rate_frozen_alert() -> Alert {
    const ALERT_NAME: &str = "eth_to_strk_rate_frozen";
    oracle_rate_frozen_alert(
        ALERT_NAME,
        "Eth to Strk rate frozen",
        &ETH_TO_STRK_RATE,
        SeverityValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
    )
}

pub(crate) fn get_strk_to_usd_rate_frozen_alert() -> Alert {
    const ALERT_NAME: &str = "strk_to_usd_rate_frozen";
    oracle_rate_frozen_alert(
        ALERT_NAME,
        "Strk to Usd rate frozen",
        &SNIP35_STRK_USD_RATE,
        SeverityValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
    )
}

/// Alert if had no successful l1 gas price scrape in the last hour.
///
/// Uses `sum_increase` for the same spot-eviction reason as `get_eth_to_strk_success_count_alert`.
pub(crate) fn get_l1_gas_price_scraper_success_count_alert() -> Alert {
    const ALERT_NAME: &str = "l1_gas_price_scraper_success_count";
    Alert::new(
        ALERT_NAME,
        "L1 gas price scraper success count",
        EvaluationRate::Default,
        sum_increase(&L1_GAS_PRICE_SCRAPER_SUCCESS_COUNT, "1h"),
        vec![AlertCondition::new(AlertComparisonOp::LessThan, 1.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        SeverityValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
        ObserverApplicability::NotApplicable,
    )
}

pub(crate) fn get_l1_gas_price_provider_insufficient_history_alert() -> Alert {
    const ALERT_NAME: &str = "l1_gas_price_provider_insufficient_history";
    Alert::new(
        ALERT_NAME,
        "L1 gas price provider insufficient history",
        EvaluationRate::Default,
        format!(
            "increase({}[1m])",
            L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        SeverityValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
        ObserverApplicability::NotApplicable,
    )
}

/// Alert if an exchange-rate oracle had no successful query in the last hour.
///
/// Uses `sum_increase` instead of bare `increase` to avoid false positives on spot eviction: when
/// a pod is evicted and rescheduled, the new pod's counter resets to 0, so a bare `increase([1h])`
/// would return 0 until the first success. `sum` aggregates across all pod series, and the
/// evicted pod's data points remain in the TSDB for the full 1h window, keeping the sum ≥ 1.
fn oracle_success_count_alert(
    name: &str,
    title: &str,
    success_count_metric: &dyn MetricQueryName,
    severity: impl Into<SeverityValueOrPlaceholder>,
) -> Alert {
    Alert::new(
        name,
        title,
        EvaluationRate::Default,
        sum_increase(success_count_metric, "1h"),
        vec![AlertCondition::new(AlertComparisonOp::LessThan, 1.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        severity,
        ObserverApplicability::NotApplicable,
    )
}

/// Alert if an exchange-rate oracle exceeded the failure threshold in the last hour.
///
/// `or vector(0)` keeps the query defined (evaluating to 0) when the metric has no samples yet,
/// so the alert stays silent instead of going to no-data before the first error is recorded.
fn oracle_error_count_alert(
    name: &str,
    title: &str,
    error_count_metric: &dyn MetricQueryName,
    severity: impl Into<SeverityValueOrPlaceholder>,
) -> Alert {
    Alert::new(
        name,
        title,
        EvaluationRate::Default,
        format!("{} or vector(0)", sum_increase(error_count_metric, "1h")),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 10.0, AlertLogicalOp::And)],
        "1m",
        severity,
        ObserverApplicability::NotApplicable,
    )
}

/// Alert if an exchange-rate oracle's rate gauge has not changed at all in the last hour.
///
/// Detects a *frozen feed*: the oracle keeps resolving successfully (so the success count and
/// last-success timestamp stay healthy) while serving a stale, unchanging price. `changes` over 1h
/// is 0 only when the value never moved across the ~4 update buckets in that window — effectively
/// impossible for a live 18-decimal rate. Unlike the error-count alert there is deliberately no
/// `or vector(0)`: an absent gauge must stay no-data (so an oracle that never resolves doesn't look
/// "frozen"); only a present-but-flat gauge trips this.
///
/// Applies to observers too: a frozen upstream feed is env-wide and observer nodes run the same
/// oracle client, so the alert should fire regardless of node role.
fn oracle_rate_frozen_alert(
    name: &str,
    title: &str,
    rate_metric: &dyn MetricQueryName,
    severity: impl Into<SeverityValueOrPlaceholder>,
) -> Alert {
    Alert::new(
        name,
        title,
        EvaluationRate::Default,
        format!("sum(changes({}[1h]))", rate_metric.get_name_with_filter()),
        vec![AlertCondition::new(AlertComparisonOp::LessThan, 1.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        severity,
        ObserverApplicability::Applicable,
    )
}
