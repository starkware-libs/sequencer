use std::time::Duration;

use validator::Validate;

use crate::config::L1EventsScraperConfig;

#[test]
fn default_config_passes_validation() {
    assert!(L1EventsScraperConfig::default().validate().is_ok());
}

#[test]
fn zero_l1_block_time_seconds_is_rejected() {
    let config =
        L1EventsScraperConfig { l1_block_time_seconds: Duration::ZERO, ..Default::default() };
    assert!(config.validate().is_err());
}

#[test]
fn sub_second_l1_block_time_seconds_is_rejected() {
    // `as_secs()` truncates to whole seconds, so a sub-second block time rounds to a zero divisor.
    let config = L1EventsScraperConfig {
        l1_block_time_seconds: Duration::from_millis(500),
        ..Default::default()
    };
    assert!(config.validate().is_err());
}
