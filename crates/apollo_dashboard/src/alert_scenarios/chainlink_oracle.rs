use apollo_l1_gas_price::metrics::{
    CHAINLINK_ORACLE_RATE_OUT_OF_BOUNDS_COUNT,
    CHAINLINK_ORACLE_STALE_FEED_COUNT,
};
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
use crate::query_builder::sum_increase;

#[cfg(test)]
#[path = "chainlink_oracle_test.rs"]
mod chainlink_oracle_test;

/// Sampling window, matching the exchange-rate oracle alerts these sit alongside.
const GUARD_SAMPLING_WINDOW: &str = "1h";

/// Held long enough that a single rejected reading stays quiet, since the client retries once per
/// lag interval and a rejection that matters persists across many of them.
const GUARD_PENDING_DURATION: &str = "10m";

/// Alert if a Chainlink feed reading was rejected for falling outside the accepted freshness
/// window. The feeds guarantee an update every 24h, so a rejection means the feed stopped
/// publishing or its `updated_at` is implausible.
///
/// The counter covers both directions. Too old is a dead or censored feed; too far in the future is
/// a clock ahead of ours or a poisoned timestamp, which the guard exists to reject because an
/// unbounded `updated_at` would otherwise read as permanently fresh.
pub(crate) fn get_chainlink_oracle_stale_feed_alert() -> Alert {
    guard_trip_alert(
        "chainlink_oracle_stale_feed",
        "Chainlink oracle feed outside the freshness window",
        &CHAINLINK_ORACLE_STALE_FEED_COUNT,
        AlertSeverity::WorkingHours,
    )
}

/// Alert if a Chainlink rate was rejected for falling outside the configured absolute sanity
/// bounds.
///
/// This is the wrong-feed-wiring and poisoned-feed detector, and it is the one guard consensus
/// cannot substitute for: validators check only that they agree with each other, and every node
/// reads the same chain state, so a feed reporting a plausible but wrong value produces unanimous
/// agreement. A trip here means the rate is frozen at its fallback and someone is publishing a
/// price the deployment considers impossible, so it earns a higher severity than the other guards.
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
/// `or vector(0)` keeps the query defined before the first trip: `sum` over an empty vector returns
/// empty rather than 0, which would put the rule in its no-data state and page. A registered
/// counter reads 0, which is the true "nothing rejected" value, so a restarted pod stays quiet.
///
/// Only the staleness and bounds guards get their own alert. Every guard trip also increments the
/// pair's `error_count`, which already carries a paging alert, so the invalid-answer and
/// contract-call guards would add a second page for a case the existing alert covers. These two
/// earn one because they name a cause the aggregate cannot: a feed that stopped publishing, and a
/// feed publishing an impossible price.
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
        format!("{} or vector(0)", sum_increase(guard_trip_metric, GUARD_SAMPLING_WINDOW)),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.0, AlertLogicalOp::And)],
        GUARD_PENDING_DURATION,
        severity,
        ObserverApplicability::Applicable,
    )
}
