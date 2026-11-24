use validator::Validate;

use crate::config::{ContextConfig, ContextStaticConfig};

#[test]
fn default_config_is_valid() {
    ContextConfig::default().validate().unwrap();
}

#[test]
fn calldata_l1_da_mode_is_rejected() {
    let config = ContextConfig {
        static_config: ContextStaticConfig { l1_da_mode: false, ..Default::default() },
        ..Default::default()
    };
    let error = config.validate().unwrap_err().to_string();
    assert!(error.contains("l1_da_mode"), "Unexpected error: {error}");
}
