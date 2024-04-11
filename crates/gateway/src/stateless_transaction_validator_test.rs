use rstest::rstest;

use starknet_api::external_transaction::ExternalTransaction;
use starknet_api::transaction::{Resource, ResourceBounds, ResourceBoundsMapping};

use crate::starknet_api_test_utils::{
    create_external_declare_tx_for_testing, create_external_deploy_account_tx_for_testing,
    create_external_invoke_tx_for_testing, non_zero_l1_resource_bounds_mapping,
    non_zero_l2_resource_bounds_mapping, zero_resource_bounds_mapping,
};
use crate::stateless_transaction_validator::{
    StatelessTransactionValidator, StatelessTransactionValidatorConfig, TransactionValidatorError,
    TransactionValidatorResult,
};

const VALIDATOR_CONFIG_FOR_TESTING: StatelessTransactionValidatorConfig =
    StatelessTransactionValidatorConfig {
        validate_non_zero_l1_gas_fee: true,
        validate_non_zero_l2_gas_fee: false,
    };

const VALIDATOR_CONFIG_FOR_L2_GAS_TESTING: StatelessTransactionValidatorConfig =
    StatelessTransactionValidatorConfig {
        validate_non_zero_l1_gas_fee: false,
        validate_non_zero_l2_gas_fee: true,
    };

#[rstest]
#[case::ignore_resource_bounds(
    StatelessTransactionValidatorConfig{
        validate_non_zero_l1_gas_fee: false,
        validate_non_zero_l2_gas_fee: false,
    },
    create_external_invoke_tx_for_testing(zero_resource_bounds_mapping()),
    Ok(())
)]
#[case::missing_l1_gas_resource_bounds(
    VALIDATOR_CONFIG_FOR_TESTING,
    create_external_invoke_tx_for_testing(ResourceBoundsMapping::default()),
    Err(TransactionValidatorError::MissingResource { resource: Resource::L1Gas })
)]
#[case::missing_l2_gas_resource_bounds(
    VALIDATOR_CONFIG_FOR_L2_GAS_TESTING,
    create_external_invoke_tx_for_testing(ResourceBoundsMapping::default()),
    Err(TransactionValidatorError::MissingResource { resource: Resource::L2Gas })
)]
#[case::zero_l1_gas_resource_bounds(
    VALIDATOR_CONFIG_FOR_TESTING,
    create_external_invoke_tx_for_testing(zero_resource_bounds_mapping()),
    Err(TransactionValidatorError::ZeroFee{
        resource: Resource::L1Gas, resource_bounds: ResourceBounds::default()
    })
)]
#[case::zero_l2_gas_resource_bounds(
    VALIDATOR_CONFIG_FOR_L2_GAS_TESTING,
    create_external_invoke_tx_for_testing(non_zero_l1_resource_bounds_mapping()),
    Err(TransactionValidatorError::ZeroFee{
        resource: Resource::L2Gas, resource_bounds: ResourceBounds::default()
    })
)]
#[case::valid_declare_tx(
    VALIDATOR_CONFIG_FOR_TESTING,
    create_external_declare_tx_for_testing(non_zero_l1_resource_bounds_mapping()),
    Ok(())
)]
#[case::valid_deploy_account_tx(
    VALIDATOR_CONFIG_FOR_TESTING,
    create_external_deploy_account_tx_for_testing(non_zero_l1_resource_bounds_mapping(),),
    Ok(())
)]
#[case::valid_invoke_tx(
    VALIDATOR_CONFIG_FOR_TESTING,
    create_external_invoke_tx_for_testing(non_zero_l1_resource_bounds_mapping()),
    Ok(())
)]
#[case::valid_l2_gas_invoke_tx(
    VALIDATOR_CONFIG_FOR_L2_GAS_TESTING,
    create_external_invoke_tx_for_testing(non_zero_l2_resource_bounds_mapping()),
    Ok(())
)]
fn test_transaction_validator(
    #[case] config: StatelessTransactionValidatorConfig,
    #[case] tx: ExternalTransaction,
    #[case] expected_result: TransactionValidatorResult<()>,
) {
    let tx_validator = StatelessTransactionValidator { config };
    let result = tx_validator.validate(&tx);

    assert_eq!(result, expected_result);
}
