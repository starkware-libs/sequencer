use validator::Validate;

use crate::config::BatcherStaticConfig;

fn static_config_with_blocked_storage_keys(blocked_storage_keys: &str) -> BatcherStaticConfig {
    let mut static_config = BatcherStaticConfig::default();
    static_config.block_builder_config.blocked_storage_keys = blocked_storage_keys.to_string();
    static_config
}

#[test]
fn blocked_storage_keys_are_validated() {
    assert!(static_config_with_blocked_storage_keys("").validate().is_ok());
    assert!(static_config_with_blocked_storage_keys("0x1, 0x2,").validate().is_ok());
    assert!(static_config_with_blocked_storage_keys("0x1,not_a_key").validate().is_err());
}
