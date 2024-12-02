use validator::Validate;

use crate::block_builder::BlockBuilderConfig;
use crate::config::BatcherConfig;

#[test]
fn test_validate_batcher_config_success() {
    let batcher_config = BatcherConfig::default();
    assert!(batcher_config.validate().is_ok());
}

#[test]
fn test_validate_batcher_config_failure() {
    let batcher_config = BatcherConfig {
        input_stream_content_buffer_size: 99,
        block_builder_config: BlockBuilderConfig { tx_chunk_size: 100, ..Default::default() },
        ..Default::default()
    };
    assert!(batcher_config.validate().is_err());
}
