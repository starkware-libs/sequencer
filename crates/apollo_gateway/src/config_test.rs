use validator::Validate;

use crate::config::StatelessTransactionValidatorConfig;

#[test]
fn stateless_transaction_validator_config_is_valid() {
    let config = StatelessTransactionValidatorConfig::default();
    assert!(config.validate().is_ok());

    let config = StatelessTransactionValidatorConfig { max_l2_gas_amount: 1, ..Default::default() };
    let error = config.validate().unwrap_err();
    assert!(error.to_string().contains("incompatible max_l2_gas_amount"))
}
