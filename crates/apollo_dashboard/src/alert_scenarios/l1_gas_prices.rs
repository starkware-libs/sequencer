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
    AlertGroup,
    AlertLogicalOp,
    ObserverApplicability,
    EVALUATION_INTERVAL_SEC_DEFAULT,
    PENDING_DURATION_DEFAULT,
};

/// Alert if we have no successful eth to strk rates data from the last hour.
fn get_eth_to_strk_success_count_alert(
    alert_severity: impl Into<SeverityValueOrPlaceholder>,
) -> Alert {
    Alert::new(
        "eth_to_strk_success_count",
        "Eth to Strk success count",
        AlertGroup::L1GasPrice,
        format!("increase({}[1h])", ETH_TO_STRK_SUCCESS_COUNT.get_name_with_filter()),
        vec![AlertCondition::new(AlertComparisonOp::LessThan, 1.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        ObserverApplicability::NotApplicable,
    )
}

pub(crate) fn get_eth_to_strk_success_count_alert_vec() -> Vec<Alert> {
    vec![get_eth_to_strk_success_count_alert(SeverityValueOrPlaceholder::Placeholder(
        "eth_to_strk_success_count".to_string(),
    ))]
}

/// Alert if had no successful l1 gas price scrape in the last hour.
fn get_l1_gas_price_scraper_success_count_alert(
    alert_severity: impl Into<SeverityValueOrPlaceholder>,
) -> Alert {
    Alert::new(
        "l1_gas_price_scraper_success_count",
        "L1 gas price scraper success count",
        AlertGroup::L1GasPrice,
        format!("increase({}[1h])", L1_GAS_PRICE_SCRAPER_SUCCESS_COUNT.get_name_with_filter()),
        vec![AlertCondition::new(AlertComparisonOp::LessThan, 1.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        ObserverApplicability::NotApplicable,
    )
}

pub(crate) fn get_l1_gas_price_scraper_success_count_alert_vec() -> Vec<Alert> {
    vec![get_l1_gas_price_scraper_success_count_alert(SeverityValueOrPlaceholder::Placeholder(
        "l1_gas_price_scraper_success_count".to_string(),
    ))]
}

fn get_l1_gas_price_provider_insufficient_history_alert(
    alert_severity: impl Into<SeverityValueOrPlaceholder>,
) -> Alert {
    Alert::new(
        "l1_gas_price_provider_insufficient_history",
        "L1 gas price provider insufficient history",
        AlertGroup::L1GasPrice,
        format!(
            "increase({}[1m])",
            L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY.get_name_with_filter()
        ),
        vec![AlertCondition::new(AlertComparisonOp::GreaterThan, 0.0, AlertLogicalOp::And)],
        PENDING_DURATION_DEFAULT,
        EVALUATION_INTERVAL_SEC_DEFAULT,
        alert_severity,
        ObserverApplicability::NotApplicable,
    )
}

pub(crate) fn get_l1_gas_price_provider_insufficient_history_alert_vec() -> Vec<Alert> {
    vec![get_l1_gas_price_provider_insufficient_history_alert(
        SeverityValueOrPlaceholder::Placeholder(
            "l1_gas_price_provider_insufficient_history".to_string(),
        ),
    )]
}
