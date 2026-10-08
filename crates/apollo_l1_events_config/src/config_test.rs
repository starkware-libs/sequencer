use super::L1EventsScraperConfig;

// Every environment deploys this default, so changing it must be a deliberate, reviewed act.
#[test]
fn default_max_l1_handler_payload_length() {
    assert_eq!(L1EventsScraperConfig::default().max_l1_handler_payload_length, 20);
}
