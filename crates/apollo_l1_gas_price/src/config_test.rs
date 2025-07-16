use std::time::Duration;

use assert_matches::assert_matches;
use rstest::rstest;
use validator::Validate;

use crate::config::L1GasPriceConfig;

#[rstest]
#[case::lag_margin_ok(L1GasPriceConfig { lag_margin_seconds: 300, finality: 10, polling_interval: Duration::from_secs(1), ..Default::default() }, None)]
#[case::lag_margin_finality_fail(L1GasPriceConfig { lag_margin_seconds: 300, finality: 30, polling_interval: Duration::from_secs(1), ..Default::default() }, Some("lag_margin_seconds=300 should be greater than 301"))]
#[case::lag_margin_polling_fail(L1GasPriceConfig { lag_margin_seconds: 300, finality: 10, polling_interval: Duration::from_secs(300), ..Default::default() }, Some("lag_margin_seconds=300 should be greater than 400"))]
#[case::lag_margin_finality_0_ok(L1GasPriceConfig { lag_margin_seconds: 300, finality: 0, polling_interval: Duration::from_secs(1), ..Default::default() }, None)]
fn validate_l1_gas_price_config(
    #[case] config: L1GasPriceConfig,
    #[case] expect_failure_string: Option<&str>,
) {
    if let Some(failure_string) = expect_failure_string {
        assert_matches!(config.validate(), Err(e) if e.to_string().contains(failure_string));
    } else {
        assert!(config.validate().is_ok());
    }
}
