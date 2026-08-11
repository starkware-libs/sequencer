use apollo_l1_gas_price_types::errors::ExchangeRateOracleErrorType;
use apollo_l1_gas_price_types::{CurrencyPair, LABEL_NAME_CURRENCY_PAIR, LABEL_NAME_ERROR_TYPE};
use serde_json::Value;

use super::{
    get_eth_to_strk_oracle_stale_alert,
    get_eth_to_strk_rate_out_of_bounds_alert,
    get_strk_to_usd_oracle_stale_alert,
    get_strk_to_usd_rate_out_of_bounds_alert,
};
use crate::alerts::Alert;

fn alert_expression(alert: &Alert) -> String {
    let alert_json: Value = serde_json::to_value(alert).expect("Alert should serialize.");
    alert_json["expr"].as_str().expect("Alert should carry an expression.").to_string()
}

/// Registration publishes every label permutation at 0, so without the `> 0` guard the staleness
/// alert would read `time() - 0` and fire forever, including for a pair no client serves.
#[test]
fn stale_alerts_ignore_a_gauge_that_never_recorded_a_success() {
    for (alert, currency_pair) in [
        (get_eth_to_strk_oracle_stale_alert(), CurrencyPair::EthStrk),
        (get_strk_to_usd_oracle_stale_alert(), CurrencyPair::StrkUsd),
    ] {
        let expression = alert_expression(&alert);
        let pair_filter = format!("{LABEL_NAME_CURRENCY_PAIR}=\"{}\"", <&str>::from(currency_pair));
        assert!(expression.contains(&pair_filter), "{expression} should select one pair.");
        assert!(expression.contains("> 0)"), "{expression} should drop an unset gauge.");
    }
}

/// The pair alone would also match the other guards, which the aggregate error-count alert already
/// covers at a threshold; this alert pages on a single rejection, so it must select one error type.
#[test]
fn rate_out_of_bounds_alerts_select_one_pair_and_one_error_type() {
    for (alert, currency_pair) in [
        (get_eth_to_strk_rate_out_of_bounds_alert(), CurrencyPair::EthStrk),
        (get_strk_to_usd_rate_out_of_bounds_alert(), CurrencyPair::StrkUsd),
    ] {
        let expression = alert_expression(&alert);
        assert!(
            expression.contains(&format!(
                "{LABEL_NAME_CURRENCY_PAIR}=\"{}\"",
                <&str>::from(currency_pair)
            )),
            "{expression} should select one pair."
        );
        assert!(
            expression.contains(&format!(
                "{LABEL_NAME_ERROR_TYPE}=\"{}\"",
                <&str>::from(ExchangeRateOracleErrorType::RateOutOfBoundsError)
            )),
            "{expression} should select the rate-out-of-bounds error type."
        );
    }
}
