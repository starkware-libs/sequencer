use apollo_consensus_orchestrator::metrics::CONSENSUS_L2_GAS_PRICE_AT_MINIMUM;
use apollo_l1_gas_price::metrics::{
    EXCHANGE_RATE_ORACLE_ERROR_COUNT,
    EXCHANGE_RATE_ORACLE_RATE,
    EXCHANGE_RATE_ORACLE_SUCCESS_COUNT,
    L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY,
    L1_GAS_PRICE_SCRAPER_SUCCESS_COUNT,
};
use apollo_l1_gas_price_types::{CurrencyPair, LABEL_NAME_CURRENCY_PAIR};
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
use crate::query_builder::{sum_increase, sum_increase_with_label, with_label};

pub(crate) fn get_eth_to_strk_success_count_alert() -> Alert {
    const ALERT_NAME: &str = "eth_to_strk_success_count";
    oracle_success_count_alert(
        ALERT_NAME,
        "Eth to Strk success count",
        CurrencyPair::EthStrk,
        SeverityValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
    )
}

pub(crate) fn get_eth_to_strk_error_count_alert() -> Alert {
    oracle_error_count_alert(
        "eth_to_strk_error_count",
        "Eth to Strk error count",
        CurrencyPair::EthStrk,
        AlertSeverity::Informational,
    )
}

pub(crate) fn get_strk_to_usd_success_count_alert() -> Alert {
    const ALERT_NAME: &str = "strk_to_usd_success_count";
    oracle_success_count_alert(
        ALERT_NAME,
        "Strk to Usd success count",
        CurrencyPair::StrkUsd,
        SeverityValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
    )
}

pub(crate) fn get_strk_to_usd_error_count_alert() -> Alert {
    oracle_error_count_alert(
        "strk_to_usd_error_count",
        "Strk to Usd error count",
        CurrencyPair::StrkUsd,
        AlertSeverity::Informational,
    )
}

pub(crate) fn get_eth_to_strk_rate_frozen_alert() -> Alert {
    const ALERT_NAME: &str = "eth_to_strk_rate_frozen";
    oracle_rate_frozen_alert(
        ALERT_NAME,
        "Eth to Strk rate frozen",
        CurrencyPair::EthStrk,
        SeverityValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
    )
}

pub(crate) fn get_strk_to_usd_rate_frozen_alert() -> Alert {
    const ALERT_NAME: &str = "strk_to_usd_rate_frozen";
    oracle_rate_frozen_alert(
        ALERT_NAME,
        "Strk to Usd rate frozen",
        CurrencyPair::StrkUsd,
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

/// Alert when the accepted L2 gas price sits at the configured minimum for a sustained window,
/// i.e. the EIP-1559 fee market has clamped the price at its floor (demand bottomed out).
///
/// Fires on the `consensus_l2_gas_price_at_minimum` gauge (1 = clamped at the configured min),
/// which the orchestrator derives per height from `min_l2_gas_price_per_height` (falling back to
/// the versioned-constants `min_gas_price`). Emitting the "at minimum" signal from the node — where
/// the configured min is known — avoids a hard-coded Grafana threshold that would rot on a
/// versioned-constants change or miss any env that overrides the min (e.g. Mainnet's 15e9).
pub(crate) fn get_l2_gas_price_at_minimum_alert() -> Alert {
    Alert::new(
        "consensus_l2_gas_price_at_minimum",
        "L2 gas price at configured minimum",
        EvaluationRate::Default,
        format!("max({})", CONSENSUS_L2_GAS_PRICE_AT_MINIMUM.get_name_with_filter()),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.0, AlertLogicalOp::And)],
        "2m",
        AlertSeverity::DayOnly,
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
    pair: CurrencyPair,
    severity: impl Into<SeverityValueOrPlaceholder>,
) -> Alert {
    Alert::new(
        name,
        title,
        EvaluationRate::Default,
        sum_increase_with_label(
            &EXCHANGE_RATE_ORACLE_SUCCESS_COUNT,
            LABEL_NAME_CURRENCY_PAIR,
            pair.into(),
            "1h",
        ),
        vec![AlertCondition::new(AlertComparisonOp::LessThan, 1.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        severity,
        ObserverApplicability::NotApplicable,
    )
}

/// Alert if an exchange-rate oracle exceeded the failure threshold in the last hour.
///
/// Sums over every `error_type` of the pair: the threshold is about how often the oracle failed,
/// not about which variant it failed with.
///
/// `or vector(0)` keeps the query defined (evaluating to 0) when the metric has no samples yet,
/// so the alert stays silent instead of going to no-data before the first error is recorded.
fn oracle_error_count_alert(
    name: &str,
    title: &str,
    pair: CurrencyPair,
    severity: impl Into<SeverityValueOrPlaceholder>,
) -> Alert {
    Alert::new(
        name,
        title,
        EvaluationRate::Default,
        format!(
            "{} or vector(0)",
            sum_increase_with_label(
                &EXCHANGE_RATE_ORACLE_ERROR_COUNT,
                LABEL_NAME_CURRENCY_PAIR,
                pair.into(),
                "1h",
            )
        ),
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
    pair: CurrencyPair,
    severity: impl Into<SeverityValueOrPlaceholder>,
) -> Alert {
    Alert::new(
        name,
        title,
        EvaluationRate::Default,
        format!(
            "sum(changes({}[1h]))",
            with_label(&EXCHANGE_RATE_ORACLE_RATE, LABEL_NAME_CURRENCY_PAIR, pair.into())
        ),
        vec![AlertCondition::new(AlertComparisonOp::LessThan, 1.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        severity,
        ObserverApplicability::Applicable,
    )
}
