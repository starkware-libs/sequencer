use validator::Validate;

use super::MempoolStaticConfig;

#[test]
fn default_static_config_passes_validation() {
    assert!(MempoolStaticConfig::default().validate().is_ok());
}

#[test]
fn zero_fee_escalation_percentage_fails_validation() {
    let static_config = MempoolStaticConfig { fee_escalation_percentage: 0, ..Default::default() };
    assert!(static_config.validate().is_err());
}
