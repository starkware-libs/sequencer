use validator::Validate;

use crate::config::L1EventsScraperConfig;

#[test]
fn zero_max_l1_handler_payload_length_fails_validation() {
    let config = L1EventsScraperConfig { max_l1_handler_payload_length: 0, ..Default::default() };
    assert!(config.validate().is_err());
}
