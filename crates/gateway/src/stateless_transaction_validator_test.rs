use rstest::rstest;

use starknet_api::calldata;
use starknet_api::external_transaction::ExternalTransaction;
use starknet_api::hash::StarkFelt;
use starknet_api::transaction::{
    Calldata, Resource, ResourceBounds, ResourceBoundsMapping, TransactionSignature,
};

use crate::starknet_api_test_utils::{
    create_resource_bounds_mapping, external_declare_tx_for_testing,
    external_deploy_account_tx_for_testing, external_invoke_tx_for_testing,
    non_zero_resource_bounds_mapping, zero_resource_bounds_mapping, NON_EMPTY_RESOURCE_BOUNDS,
};
use crate::stateless_transaction_validator::{
    StatelessTransactionValidator, StatelessTransactionValidatorConfig,
    StatelessTransactionValidatorError, StatelessTransactionValidatorResult,
};

const DEFAULT_VALIDATOR_CONFIG_FOR_TESTING: StatelessTransactionValidatorConfig =
    StatelessTransactionValidatorConfig {
        validate_non_zero_l1_gas_fee: true,
        validate_non_zero_l2_gas_fee: true,

        max_calldata_length: 1,
        max_signature_length: 1,
    };

#[rstest]
// Resource bounds validation tests.
#[case::ignore_resource_bounds(
    StatelessTransactionValidatorConfig{
        validate_non_zero_l1_gas_fee: false,
        validate_non_zero_l2_gas_fee: false,
        ..DEFAULT_VALIDATOR_CONFIG_FOR_TESTING
    },
    external_invoke_tx_for_testing(
        zero_resource_bounds_mapping(), calldata![], TransactionSignature::default()
    ),
    Ok(())
)]
#[case::missing_l1_gas_resource_bounds(
    StatelessTransactionValidatorConfig {
        validate_non_zero_l1_gas_fee: true,
        validate_non_zero_l2_gas_fee: false,
        ..DEFAULT_VALIDATOR_CONFIG_FOR_TESTING
    },
    external_invoke_tx_for_testing(
        ResourceBoundsMapping::default(), calldata![], TransactionSignature::default()
    ),
    Err(StatelessTransactionValidatorError::MissingResource { resource: Resource::L1Gas })
)]
#[case::missing_l2_gas_resource_bounds(
    StatelessTransactionValidatorConfig {
        validate_non_zero_l1_gas_fee: false,
        validate_non_zero_l2_gas_fee: true,
        ..DEFAULT_VALIDATOR_CONFIG_FOR_TESTING
    },
    external_invoke_tx_for_testing(
        ResourceBoundsMapping::default(), calldata![], TransactionSignature::default()
    ),
    Err(StatelessTransactionValidatorError::MissingResource { resource: Resource::L2Gas })
)]
#[case::zero_l1_gas_resource_bounds(
    DEFAULT_VALIDATOR_CONFIG_FOR_TESTING,
    external_invoke_tx_for_testing(
        zero_resource_bounds_mapping(), calldata![], TransactionSignature::default()
    ),
    Err(StatelessTransactionValidatorError::ZeroResourceBounds{
        resource: Resource::L1Gas, resource_bounds: ResourceBounds::default()
    })
)]
#[case::zero_l2_gas_resource_bounds(
    DEFAULT_VALIDATOR_CONFIG_FOR_TESTING,
    external_invoke_tx_for_testing(
        create_resource_bounds_mapping(NON_EMPTY_RESOURCE_BOUNDS, ResourceBounds::default()),
        calldata![],
        TransactionSignature::default()
    ),
    Err(StatelessTransactionValidatorError::ZeroResourceBounds{
        resource: Resource::L2Gas, resource_bounds: ResourceBounds::default()
    })
)]
#[case::valid_l2_gas_invoke_tx(
    StatelessTransactionValidatorConfig{
        validate_non_zero_l1_gas_fee: false,
        validate_non_zero_l2_gas_fee: true,
        ..DEFAULT_VALIDATOR_CONFIG_FOR_TESTING
    },
    external_invoke_tx_for_testing(
        create_resource_bounds_mapping(ResourceBounds::default(), NON_EMPTY_RESOURCE_BOUNDS),
        calldata![],
        TransactionSignature::default()
    ),
    Ok(())
)]
// Transaction size validation tests.
#[case::deploy_account_calldata_too_long(
    DEFAULT_VALIDATOR_CONFIG_FOR_TESTING,
    external_deploy_account_tx_for_testing(
        non_zero_resource_bounds_mapping(),
        calldata![StarkFelt::from_u128(1), StarkFelt::from_u128(2)],
        TransactionSignature::default()
    ),
    Err(StatelessTransactionValidatorError::CalldataTooLong { calldata_length: 2, max_calldata_length: 1 })
)]
#[case::invoke_calldata_too_long(
    DEFAULT_VALIDATOR_CONFIG_FOR_TESTING,
    external_invoke_tx_for_testing(
        non_zero_resource_bounds_mapping(),
        calldata![StarkFelt::from_u128(1), StarkFelt::from_u128(2)],
        TransactionSignature::default()
    ),
    Err(StatelessTransactionValidatorError::CalldataTooLong { calldata_length: 2, max_calldata_length: 1 })
)]
#[case::non_empty_valid_calldata(
    DEFAULT_VALIDATOR_CONFIG_FOR_TESTING,
    external_invoke_tx_for_testing(
        non_zero_resource_bounds_mapping(),
        calldata![StarkFelt::from_u128(1)],
        TransactionSignature::default()
    ),
    Ok(())
)]
#[case::declare_signature_too_long(
    DEFAULT_VALIDATOR_CONFIG_FOR_TESTING,
    external_declare_tx_for_testing(
        non_zero_resource_bounds_mapping(),
        TransactionSignature(vec![StarkFelt::from_u128(1), StarkFelt::from_u128(2)]),
    ),
    Err(StatelessTransactionValidatorError::SignatureTooLong { signature_length: 2, max_signature_length: 1 })

)]
#[case::deploy_account_signature_too_long(
    DEFAULT_VALIDATOR_CONFIG_FOR_TESTING,
    external_deploy_account_tx_for_testing(
        non_zero_resource_bounds_mapping(),
        calldata![],
        TransactionSignature(vec![StarkFelt::from_u128(1), StarkFelt::from_u128(2)])
    ),
    Err(StatelessTransactionValidatorError::SignatureTooLong { signature_length: 2, max_signature_length: 1 })
)]
#[case::invoke_signature_too_long(
    DEFAULT_VALIDATOR_CONFIG_FOR_TESTING,
    external_invoke_tx_for_testing(
        non_zero_resource_bounds_mapping(),
        calldata![],
        TransactionSignature(vec![StarkFelt::from_u128(1), StarkFelt::from_u128(2)])
    ),
    Err(StatelessTransactionValidatorError::SignatureTooLong { signature_length: 2, max_signature_length: 1 })
)]
#[case::non_empty_valid_signature(
    DEFAULT_VALIDATOR_CONFIG_FOR_TESTING,
    external_invoke_tx_for_testing(
        non_zero_resource_bounds_mapping(),
        calldata![],
        TransactionSignature(vec![StarkFelt::from_u128(1)])
    ),
    Ok(())
)]
// General cases.
#[case::valid_declare_tx(
    DEFAULT_VALIDATOR_CONFIG_FOR_TESTING,
    external_declare_tx_for_testing(
        non_zero_resource_bounds_mapping(),
        TransactionSignature::default()
    ),
    Ok(())
)]
#[case::valid_deploy_account_tx(
    DEFAULT_VALIDATOR_CONFIG_FOR_TESTING,
    external_deploy_account_tx_for_testing(
        non_zero_resource_bounds_mapping(),
        calldata![],
        TransactionSignature::default()
    ),
    Ok(())
)]
#[case::valid_invoke_tx(
    DEFAULT_VALIDATOR_CONFIG_FOR_TESTING,
    external_invoke_tx_for_testing(
        non_zero_resource_bounds_mapping(),
        calldata![],
        TransactionSignature::default()
    ),
    Ok(())
)]
fn test_transaction_validator(
    #[case] config: StatelessTransactionValidatorConfig,
    #[case] tx: ExternalTransaction,
    #[case] expected_result: StatelessTransactionValidatorResult<()>,
) {
    let tx_validator = StatelessTransactionValidator { config };
    let result = tx_validator.validate(&tx);

    assert_eq!(result, expected_result);
}
