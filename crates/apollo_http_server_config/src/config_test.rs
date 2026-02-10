use validator::Validate;

use crate::config::{HttpServerConfig, HttpServerDynamicConfig, HttpServerStaticConfig};

#[test]
fn validate_config() {
    let config = HttpServerConfig {
        dynamic_config: HttpServerDynamicConfig {
            max_sierra_program_size: 3,
            ..Default::default()
        },
        static_config: HttpServerStaticConfig { max_request_body_size: 2, ..Default::default() },
    };

    let error = config.validate().unwrap_err();
    assert!(
        error
            .to_string()
            .contains("max_request_body_size must be greater than max_sierra_program_size")
    );
}
