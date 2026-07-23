use validator::Validate;

use super::ConsensusDynamicConfig;

#[test]
fn far_behind_proposal_threshold_default_is_valid() {
    let config = ConsensusDynamicConfig::default();
    assert_eq!(config.far_behind_proposal_threshold, 30);
    assert!(config.validate().is_ok());
}

#[test]
fn far_behind_proposal_threshold_must_be_in_range_5_to_1000() {
    // Must be at least 5 and at most 1000 (inclusive bounds).
    for valid in [5, 30, 1000] {
        let config =
            ConsensusDynamicConfig { far_behind_proposal_threshold: valid, ..Default::default() };
        assert!(config.validate().is_ok(), "{valid} should be accepted");
    }
    for invalid in [4, 1001] {
        let config =
            ConsensusDynamicConfig { far_behind_proposal_threshold: invalid, ..Default::default() };
        assert!(config.validate().is_err(), "{invalid} should be rejected");
    }
}
