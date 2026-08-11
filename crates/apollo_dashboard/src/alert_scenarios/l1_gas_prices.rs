use apollo_consensus_orchestrator::metrics::CONSENSUS_L2_GAS_PRICE_AT_MINIMUM;
use apollo_l1_gas_price::metrics::{
    EXCHANGE_RATE_ORACLE_ERROR_COUNT,
    EXCHANGE_RATE_ORACLE_LAST_SUCCESS_TIMESTAMP_SECONDS,
    EXCHANGE_RATE_ORACLE_RATE,
    L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY,
    L1_GAS_PRICE_SCRAPER_SUCCESS_COUNT,
};
use apollo_l1_gas_price_types::errors::ExchangeRateOracleErrorType;
use apollo_l1_gas_price_types::{CurrencyPair, LABEL_NAME_CURRENCY_PAIR, LABEL_NAME_ERROR_TYPE};
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
use crate::query_builder::{
    sum_increase,
    sum_increase_with_label,
    sum_increase_with_labels,
    with_label,
};

#[cfg(test)]
#[path = "l1_gas_prices_test.rs"]
mod l1_gas_prices_test;

/// How long a pair may go without a successful oracle query before paging: three refreshes at the
/// slowest supported cadence. Both oracles refresh every 900s in production, the HTTP one on
/// `*_oracle_config.lag_interval_seconds` and the Chainlink one on
/// `chainlink_oracle_config.sampling_interval_seconds`, both set in
/// `apollo_deployments/resources/app_configs/l1_gas_price_provider_config.json` rather than by
/// their schema defaults. Raising either past 900 without raising this makes a healthy oracle look
/// stale.
const ORACLE_STALENESS_THRESHOLD_SECONDS: f64 = 2700.0;

pub(crate) fn get_eth_to_strk_oracle_stale_alert() -> Alert {
    // Kept as `eth_to_strk_success_count`: the name is the key the per-env severity overrides set.
    const ALERT_NAME: &str = "eth_to_strk_success_count";
    oracle_stale_alert(
        ALERT_NAME,
        "Eth to Strk oracle produced no rate",
        CurrencyPair::EthStrk,
        SeverityValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
    )
}

pub(crate) fn get_eth_to_strk_rate_out_of_bounds_alert() -> Alert {
    const ALERT_NAME: &str = "eth_to_strk_rate_out_of_bounds";
    oracle_rate_out_of_bounds_alert(
        ALERT_NAME,
        "Eth to Strk rate outside the configured bounds",
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

pub(crate) fn get_strk_to_usd_oracle_stale_alert() -> Alert {
    // Kept as `strk_to_usd_success_count`: the name is the key the per-env severity overrides set.
    const ALERT_NAME: &str = "strk_to_usd_success_count";
    oracle_stale_alert(
        ALERT_NAME,
        "Strk to Usd oracle produced no rate",
        CurrencyPair::StrkUsd,
        SeverityValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
    )
}

pub(crate) fn get_strk_to_usd_rate_out_of_bounds_alert() -> Alert {
    const ALERT_NAME: &str = "strk_to_usd_rate_out_of_bounds";
    oracle_rate_out_of_bounds_alert(
        ALERT_NAME,
        "Strk to Usd rate outside the configured bounds",
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
/// `sum_increase` rather than bare `increase`: a rescheduled pod's counter restarts at 0, and
/// summing across the pod series keeps the evicted pod's samples counted for the full window.
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

/// Alert if a pair has had no successful oracle query for
/// [`ORACLE_STALENESS_THRESHOLD_SECONDS`].
///
/// Reads the last-success gauge rather than counting successes, so the alert does not depend on how
/// often the client queries: any cadence below the threshold satisfies it, and a cadence that slows
/// past the threshold pages instead of going silent.
///
/// `max` is over ages, so the stalest replica governs rather than the freshest: every node serves
/// proposals from the rate its own client produced, and one node stuck on a fallback rate is worth
/// paging for.
///
/// `and (... > 0)` drops a series that never recorded a success, including the pairs no client
/// serves: registration publishes every label permutation at 0, and `time() - 0` would page
/// forever.
fn oracle_stale_alert(
    name: &str,
    title: &str,
    pair: CurrencyPair,
    severity: impl Into<SeverityValueOrPlaceholder>,
) -> Alert {
    let last_success = with_label(
        &EXCHANGE_RATE_ORACLE_LAST_SUCCESS_TIMESTAMP_SECONDS,
        LABEL_NAME_CURRENCY_PAIR,
        pair.into(),
    );
    Alert::new(
        name,
        title,
        EvaluationRate::Default,
        format!("max((time() - {last_success}) and ({last_success} > 0))"),
        vec![AlertCondition::new(
            AlertComparisonOp::GreaterThan,
            ORACLE_STALENESS_THRESHOLD_SECONDS,
            AlertLogicalOp::And,
        )],
        PENDING_DURATION_DEFAULT,
        severity,
        ObserverApplicability::NotApplicable,
    )
}

/// Alert if a rate was rejected for falling outside the configured absolute bounds.
///
/// The wrong-feed-wiring and poisoned-answer detector, and the one rejection consensus cannot
/// substitute for: validators check that they agree with each other, never that the agreed value is
/// sane. A single trip is worth paging for, so this deliberately keeps firing for the rest of the
/// window after one increment.
///
/// Applies to observers, which read the same feeds, so a trip is environment-wide either way.
fn oracle_rate_out_of_bounds_alert(
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
            sum_increase_with_labels(
                &EXCHANGE_RATE_ORACLE_ERROR_COUNT,
                &[
                    (LABEL_NAME_CURRENCY_PAIR, pair.into()),
                    (
                        LABEL_NAME_ERROR_TYPE,
                        ExchangeRateOracleErrorType::RateOutOfBoundsError.into()
                    ),
                ],
                "1h",
            )
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        severity,
        ObserverApplicability::Applicable,
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
