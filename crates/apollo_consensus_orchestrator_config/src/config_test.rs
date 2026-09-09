use validator::Validate;

use super::{ContextDynamicConfig, DEFAULT_MAX_ETH_TO_FRI_RATE_CHANGE_PPT, PPT_DENOMINATOR};

#[test]
fn default_dynamic_config_is_valid() {
    assert!(ContextDynamicConfig::default().validate().is_ok());
}

#[test]
fn max_eth_to_fri_rate_change_ppt_outside_the_band_is_rejected() {
    for max_eth_to_fri_rate_change_ppt in [0, PPT_DENOMINATOR, PPT_DENOMINATOR + 1] {
        let config = ContextDynamicConfig { max_eth_to_fri_rate_change_ppt, ..Default::default() };
        assert!(
            config.validate().is_err(),
            "accepted max_eth_to_fri_rate_change_ppt={max_eth_to_fri_rate_change_ppt}"
        );
    }
    for max_eth_to_fri_rate_change_ppt in
        [1, DEFAULT_MAX_ETH_TO_FRI_RATE_CHANGE_PPT, PPT_DENOMINATOR - 1]
    {
        let config = ContextDynamicConfig { max_eth_to_fri_rate_change_ppt, ..Default::default() };
        assert!(
            config.validate().is_ok(),
            "rejected max_eth_to_fri_rate_change_ppt={max_eth_to_fri_rate_change_ppt}"
        );
    }
}
