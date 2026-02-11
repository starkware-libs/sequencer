use rstest::rstest;
use validator::Validate;

use crate::config::{HttpServerConfig, HttpServerDynamicConfig, HttpServerStaticConfig};

#[rstest]
#[case::valid(2, 3, true)]
#[case::invalid(3, 2, false)]
fn validate_config(
    #[case] max_sierra_program_size: usize,
    #[case] max_request_body_size: usize,
    #[case] is_valid: bool,
) {
    let config = HttpServerConfig {
        dynamic_config: HttpServerDynamicConfig { max_sierra_program_size, ..Default::default() },
        static_config: HttpServerStaticConfig { max_request_body_size, ..Default::default() },
    };

    assert_eq!(
        config.validate().is_ok(),
        is_valid,
        "unexpected validation result: config {config:?}, is_valid: {is_valid}"
    );
}
