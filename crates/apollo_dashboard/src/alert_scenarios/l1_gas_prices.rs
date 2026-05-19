use apollo_l1_gas_price::metrics::{
    ETH_TO_STRK_SUCCESS_COUNT,
    L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY,
    L1_GAS_PRICE_SCRAPER_SUCCESS_COUNT,
};
use apollo_metrics::metrics::MetricQueryName;

use crate::alert_placeholders::SeverityValueOrPlaceholder;
use crate::alerts::{
    Alert,
    AlertComparisonOp,
    AlertCondition,
    AlertLogicalOp,
    EvaluationRate,
    ObserverApplicability,
    PENDING_DURATION_DEFAULT,
};
use crate::query_builder::sum_increase;

/// Alert if we have no successful eth to strk rates data from the last hour.
///
/// Uses `sum_increase` instead of bare `increase` to avoid false positives on spot eviction: when
/// a pod is evicted and rescheduled, the new pod's counter resets to 0, so a bare `increase([1h])`
/// would return 0 until the first success. `sum` aggregates across all pod series, and the
/// evicted pod's data points remain in the TSDB for the full 1h window, keeping the sum ≥ 1.
pub(crate) fn get_eth_to_strk_success_count_alert() -> Alert {
    const ALERT_NAME: &str = "eth_to_strk_success_count";
    Alert::new(
        ALERT_NAME,
        "Eth to Strk success count",
        EvaluationRate::Default,
        sum_increase(&ETH_TO_STRK_SUCCESS_COUNT, "1h"),
        vec![AlertCondition::new(AlertComparisonOp::LessThan, 1.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        SeverityValueOrPlaceholder::Placeholder(ALERT_NAME.to_string()),
        ObserverApplicability::NotApplicable,
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

#[cfg(test)]
mod tests {
    use apollo_metrics::metric_definitions::{METRIC_LABEL_FILTER, POD_LABEL_FILTER};

    use super::{get_eth_to_strk_success_count_alert, get_l1_gas_price_scraper_success_count_alert};

    // Verifies that success-count alert queries aggregate with sum() so that a pod restart after
    // spot eviction does not generate a false positive (the evicted pod's data stays in the TSDB
    // for the full 1h window, keeping the sum ≥ 1 while the new pod's counter is still at 0).
    #[test]
    fn success_count_alerts_use_sum_increase() {
        // Alert serialization strips the pod filter (Grafana alert evaluation doesn't substitute
        // $pod), so construct the expected filter without it.
        let alert_filter = METRIC_LABEL_FILTER.replace(POD_LABEL_FILTER, "");
        for (alert, metric_name) in [
            (get_eth_to_strk_success_count_alert(), "eth_to_strk_success_count"),
            (
                get_l1_gas_price_scraper_success_count_alert(),
                "l1_gas_price_scraper_success_count",
            ),
        ] {
            let serialized = serde_json::to_value(&alert).unwrap();
            let expr = serialized["expr"].as_str().unwrap();
            let expected_inner = format!("sum(increase({metric_name}{alert_filter}[1h]))");
            assert!(
                expr.starts_with(&format!("({expected_inner})")),
                "Expected expr to start with '({expected_inner})', got: {expr}"
            );
        }
    }
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
