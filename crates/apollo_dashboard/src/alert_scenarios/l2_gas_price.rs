use apollo_consensus_orchestrator::metrics::{
    L2GasPriceClampBound,
    CONSENSUS_L2_GAS_PRICE,
    CONSENSUS_L2_GAS_PRICE_AT_MINIMUM,
    CONSENSUS_L2_GAS_PRICE_CLAMPED,
    LABEL_L2_GAS_PRICE_CLAMP_BOUND,
    SNIP35_FEE_TARGET_ABOVE_MAXIMUM,
};
use apollo_metrics::metrics::MetricQueryName;

use crate::alert_placeholders::ComparisonValueOrPlaceholder;
use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertLogicalOp,
    AlertSeverity,
    EvaluationRate,
    ObserverApplicability,
};
use crate::query_builder::{sum_increase, sum_increase_with_label};

#[cfg(test)]
#[path = "l2_gas_price_test.rs"]
mod l2_gas_price_test;

/// Sampling window for the counters below, kept shorter than the pending duration of the alerts
/// that use it so that a single clamped block stops counting before it can page.
const COUNTER_SAMPLING_WINDOW: &str = "5m";

/// Alert while the accepted L2 gas price rests at the configured minimum, i.e. the EIP-1559 fee
/// market bottomed out.
///
/// Reads the `consensus_l2_gas_price_at_minimum` indicator, not the `minimum` clamp counter (which
/// instead counts blocks still ramping up towards the floor). The node emits it because the
/// configured minimum is known there, so the alert follows overrides and versioned-constants
/// changes.
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

/// Alert while the computed L2 gas price keeps running into the ceiling
/// (`MAX_GAS_PRICE_MULTIPLIER` times the configured minimum): either the price itself is capped
/// there, or the SNIP-35 floor alone has risen above it while the published price ramps up to
/// match.
///
/// Reads the node's own comparison against the ceiling, so the alert inherits whatever bound the
/// node enforces. Applies to observers: every node derives the same price per block.
pub(crate) fn get_l2_gas_price_above_maximum_alert() -> Alert {
    let above_maximum_query = sum_increase_with_label(
        &CONSENSUS_L2_GAS_PRICE_CLAMPED,
        LABEL_L2_GAS_PRICE_CLAMP_BOUND,
        L2GasPriceClampBound::Maximum.into(),
        COUNTER_SAMPLING_WINDOW,
    );
    Alert::new(
        "consensus_l2_gas_price_above_maximum",
        "L2 gas price computation above the configured maximum",
        EvaluationRate::Default,
        format!("{above_maximum_query} or vector(0)"),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.0, AlertLogicalOp::And)],
        "10m",
        AlertSeverity::Regular,
        ObserverApplicability::Applicable,
    )
}

/// Alert while the oracle-derived `fee_target` keeps exceeding the ceiling, which pins this node's
/// `fee_proposal` to the maximum.
///
/// Separate from `get_l2_gas_price_above_maximum_alert` because it names the cause: a STRK/USD feed
/// reporting STRK far too cheap, rather than congestion, so the remedy is the feed not the traffic.
pub(crate) fn get_snip35_fee_target_above_maximum_alert() -> Alert {
    Alert::new(
        "snip35_fee_target_above_maximum",
        "SNIP-35 fee target above the L2 gas price maximum",
        EvaluationRate::Default,
        format!(
            "{} or vector(0)",
            sum_increase(&SNIP35_FEE_TARGET_ABOVE_MAXIMUM, COUNTER_SAMPLING_WINDOW)
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.0, AlertLogicalOp::And)],
        "10m",
        AlertSeverity::WorkingHours,
        ObserverApplicability::Applicable,
    )
}

/// Alert while the accepted L2 gas price is far above the configured minimum but still inside the
/// band: an early warning before the ceiling.
///
/// The threshold is a placeholder because the minimum is per-deployment configuration
/// (`min_l2_gas_price_per_height`, falling back to the versioned-constants `min_gas_price`).
///
/// Reads the gauge unfiltered: `max()` ignores a restarting pod's registered 0, but a `> 0` filter
/// would empty the vector on a cold start and fire `NoData`.
pub(crate) fn get_l2_gas_price_far_above_minimum_alert() -> Alert {
    const ALERT_NAME: &str = "consensus_l2_gas_price_far_above_minimum";
    Alert::new(
        ALERT_NAME,
        "L2 gas price far above the configured minimum",
        EvaluationRate::Default,
        format!("max({})", CONSENSUS_L2_GAS_PRICE.get_name_with_filter()),
        vec![AlertCondition::new(
            AlertComparisonOp::GreaterThan,
            ComparisonValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
            AlertLogicalOp::And,
        )],
        "30m",
        AlertSeverity::WorkingHours,
        ObserverApplicability::Applicable,
    )
}
