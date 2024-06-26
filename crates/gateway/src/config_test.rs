use assert_matches::assert_matches;
use validator::Validate;

use super::StatelessTransactionValidatorConfig;
use crate::compiler_version::VersionId;

#[test]
fn test_stateless_transaction_validator_config_validation() {
    let mut config = StatelessTransactionValidatorConfig {
        max_sierra_version: VersionId { major: 1, minor: 2, patch: 0 },
        ..Default::default()
    };
    assert_matches!(config.validate(), Ok(()));

    config.max_sierra_version.patch = 1;
    assert!(config.validate().is_err());
}
