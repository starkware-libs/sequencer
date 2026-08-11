use apollo_l1_gas_price::metrics::{
    CHAINLINK_ORACLE_FUTURE_FEED_COUNT,
    CHAINLINK_ORACLE_RATE_OUT_OF_BOUNDS_COUNT,
    CHAINLINK_ORACLE_STALE_FEED_COUNT,
};
use apollo_l1_gas_price_types::LABEL_NAME_CURRENCY_PAIR;
use apollo_metrics::metrics::MetricQueryName;

use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertLogicalOp,
    AlertSeverity,
    EvaluationRate,
    ObserverApplicability,
};
use crate::query_builder::{sum_by_label, DisplayMethod};

#[cfg(test)]
#[path = "chainlink_oracle_test.rs"]
mod chainlink_oracle_test;

/// Sampling window for the guard counters, deliberately shorter than [`GUARD_PENDING_DURATION`]: a
/// trip holds the condition true for one window and then releases it, so a lone trip cannot hold
/// the condition for the whole pending duration and page.
const GUARD_SAMPLING_WINDOW: &str = "5m";

/// Outlasts [`GUARD_SAMPLING_WINDOW`], so the condition has to be re-satisfied by fresh trips for
/// the whole duration before the alert fires. The client retries once per lag interval, so a guard
/// rejecting persistently trips several times per window and holds the condition continuously.
const GUARD_PENDING_DURATION: &str = "10m";

/// Alert if a Chainlink feed reading was rejected for being older than the accepted staleness
/// bound. The feeds guarantee an update every 24h, so a rejection means the feed stopped publishing
/// or its `updated_at` is implausible.
pub(crate) fn get_chainlink_oracle_stale_feed_alert() -> Alert {
    guard_trip_alert(
        "chainlink_oracle_stale_feed",
        "Chainlink oracle feed outside the freshness window",
        &CHAINLINK_ORACLE_STALE_FEED_COUNT,
        AlertSeverity::WorkingHours,
    )
}

/// Alert if a Chainlink feed reading was rejected for being dated ahead of the block being priced:
/// a clock ahead of ours, or a poisoned timestamp, which the guard rejects because an unbounded
/// `updated_at` alone reads as permanently fresh. The same class of failure as a stale feed, hence
/// the same severity.
pub(crate) fn get_chainlink_oracle_future_feed_alert() -> Alert {
    guard_trip_alert(
        "chainlink_oracle_future_feed",
        "Chainlink oracle feed dated ahead of the block",
        &CHAINLINK_ORACLE_FUTURE_FEED_COUNT,
        AlertSeverity::WorkingHours,
    )
}

/// Alert if a Chainlink rate was rejected for falling outside the configured absolute sanity
/// bounds.
///
/// The wrong-feed-wiring and poisoned-feed detector, and the one guard consensus cannot substitute
/// for: validators only check that they agree with each other, not that the value is sane. A trip
/// means the rate is frozen at its fallback and someone is publishing a price the deployment
/// considers impossible, hence the higher severity than the other guards.
pub(crate) fn get_chainlink_oracle_rate_out_of_bounds_alert() -> Alert {
    guard_trip_alert(
        "chainlink_oracle_rate_out_of_bounds",
        "Chainlink oracle rate outside the configured bounds",
        &CHAINLINK_ORACLE_RATE_OUT_OF_BOUNDS_COUNT,
        AlertSeverity::DayOnly,
    )
}

/// Alert while a Chainlink guard keeps rejecting readings.
///
/// Grouped by currency pair, so the page names the rejected reading. The derived ETH/STRK rate
/// reads both USD feeds, and one alert instance per pair is what separates a dead ETH/USD feed from
/// a dead STRK/USD one.
///
/// `or vector(0)` keeps the query defined before the first trip: `sum` over an empty vector returns
/// empty rather than 0, which puts the rule into its no-data state and pages. A registered counter
/// reads 0, which is the true "nothing rejected" value, so a restarted pod stays quiet. The
/// fallback carries no `currency_pair` label, so once the counter exists it joins the per-pair
/// instances as an unlabeled one that holds at 0 and never fires.
///
/// The two freshness guards and the bounds guard get their own alert; the invalid-answer and
/// contract-call guards do not. Every guard trip also increments the pair's `error_count`, which
/// already carries a paging alert, so an alert per guard is a second page for a case that one
/// covers. These three earn it because they name a cause the aggregate cannot: a feed that stopped
/// publishing, a feed dated ahead of the chain, and a feed publishing an impossible price. An
/// invalid answer or a failed contract call names nothing beyond "the read failed", which is what
/// `error_count` already says.
///
/// Applies to observers, unlike the exchange-rate oracle alerts alongside it. `NotApplicable`
/// appends `and on() (is_observer == 0)` around the whole expression, which would discard the
/// `or vector(0)` fallback and return the rule to the no-data state wherever `is_observer` is
/// absent. Observers read the same feeds, so a trip is environment-wide either way.
fn guard_trip_alert(
    name: &str,
    title: &str,
    guard_trip_metric: &dyn MetricQueryName,
    severity: AlertSeverity,
) -> Alert {
    Alert::new(
        name,
        title,
        EvaluationRate::Default,
        format!(
            "{} or vector(0)",
            sum_by_label(
                guard_trip_metric,
                LABEL_NAME_CURRENCY_PAIR,
                DisplayMethod::Increase(GUARD_SAMPLING_WINDOW),
                false,
            )
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.0, AlertLogicalOp::And)],
        GUARD_PENDING_DURATION,
        severity,
        ObserverApplicability::Applicable,
    )
}
