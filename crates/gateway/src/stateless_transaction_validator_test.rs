use assert_matches::assert_matches;
use rstest::rstest;
use starknet_api::calldata;
use starknet_api::external_transaction::ResourceBoundsMapping;
use starknet_api::hash::StarkFelt;
use starknet_api::transaction::{Calldata, Resource, ResourceBounds, TransactionSignature};

use crate::starknet_api_test_utils::{
    create_resource_bounds_mapping,
    external_tx_for_testing,
    non_zero_resource_bounds_mapping,
    zero_resource_bounds_mapping,
    TransactionType,
    NON_EMPTY_RESOURCE_BOUNDS,
};
use crate::stateless_transaction_validator::{
    StatelessTransactionValidator,
    StatelessTransactionValidatorConfig,
    StatelessTransactionValidatorError,
};

const DEFAULT_VALIDATOR_CONFIG_FOR_TESTING: StatelessTransactionValidatorConfig =
    StatelessTransactionValidatorConfig {
        validate_non_zero_l1_gas_fee: true,
        validate_non_zero_l2_gas_fee: true,

        max_calldata_length: 1,
        max_signature_length: 1,
    };

#[rstest]
#[case::ignore_resource_bounds(
    StatelessTransactionValidatorConfig{
        validate_non_zero_l1_gas_fee: false,
        validate_non_zero_l2_gas_fee: false,
        ..DEFAULT_VALIDATOR_CONFIG_FOR_TESTING
    },
    zero_resource_bounds_mapping(),
    calldata![],
    TransactionSignature::default()
)]
#[case::valid_l2_gas_invoke_tx(
    StatelessTransactionValidatorConfig{
        validate_non_zero_l1_gas_fee: false,
        validate_non_zero_l2_gas_fee: true,
        ..DEFAULT_VALIDATOR_CONFIG_FOR_TESTING
    },
    create_resource_bounds_mapping(ResourceBounds::default(), NON_EMPTY_RESOURCE_BOUNDS),
    calldata![],
    TransactionSignature::default()
)]
#[case::non_empty_valid_calldata(
    DEFAULT_VALIDATOR_CONFIG_FOR_TESTING,
    non_zero_resource_bounds_mapping(),
    calldata![StarkFelt::from_u128(1)],
    TransactionSignature::default()
)]
#[case::non_empty_valid_signature(
    DEFAULT_VALIDATOR_CONFIG_FOR_TESTING,
    non_zero_resource_bounds_mapping(),
    calldata![],
    TransactionSignature(vec![StarkFelt::from_u128(1)])
)]
#[case::valid_tx(
    DEFAULT_VALIDATOR_CONFIG_FOR_TESTING,
    non_zero_resource_bounds_mapping(),
    calldata![],
    TransactionSignature::default()
)]
fn test_positive_flow(
    #[case] config: StatelessTransactionValidatorConfig,
    #[case] resource_bounds: ResourceBoundsMapping,
    #[case] tx_calldata: Calldata,
    #[case] signature: TransactionSignature,
    #[values(TransactionType::Declare, TransactionType::DeployAccount, TransactionType::Invoke)]
    tx_type: TransactionType,
) {
    let tx_validator = StatelessTransactionValidator { config };
    let tx = external_tx_for_testing(tx_type, resource_bounds, tx_calldata, signature);

    assert_matches!(tx_validator.validate(&tx), Ok(()));
}

#[rstest]
#[case::zero_l1_gas_resource_bounds(
    DEFAULT_VALIDATOR_CONFIG_FOR_TESTING,
    zero_resource_bounds_mapping(),
    StatelessTransactionValidatorError::ZeroResourceBounds{
        resource: Resource::L1Gas, resource_bounds: ResourceBounds::default()
    }
)]
#[case::zero_l2_gas_resource_bounds(
    DEFAULT_VALIDATOR_CONFIG_FOR_TESTING,
    create_resource_bounds_mapping(NON_EMPTY_RESOURCE_BOUNDS, ResourceBounds::default()),
    StatelessTransactionValidatorError::ZeroResourceBounds{
        resource: Resource::L2Gas, resource_bounds: ResourceBounds::default()
    }
)]
fn test_invalid_resource_bounds(
    #[case] config: StatelessTransactionValidatorConfig,
    #[case] resource_bounds: ResourceBoundsMapping,
    #[case] expected_error: StatelessTransactionValidatorError,
    #[values(TransactionType::Declare, TransactionType::DeployAccount, TransactionType::Invoke)]
    tx_type: TransactionType,
) {
    let tx_validator = StatelessTransactionValidator { config };
    let tx = external_tx_for_testing(
        tx_type,
        resource_bounds,
        calldata![],
        TransactionSignature::default(),
    );

    assert_eq!(tx_validator.validate(&tx).unwrap_err(), expected_error);
}

#[rstest]
fn test_calldata_too_long(
    #[values(TransactionType::DeployAccount, TransactionType::Invoke)] tx_type: TransactionType,
) {
    let tx_validator =
        StatelessTransactionValidator { config: DEFAULT_VALIDATOR_CONFIG_FOR_TESTING };
    let tx = external_tx_for_testing(
        tx_type,
        non_zero_resource_bounds_mapping(),
        calldata![StarkFelt::from_u128(1), StarkFelt::from_u128(2)],
        TransactionSignature::default(),
    );

    assert_eq!(
        tx_validator.validate(&tx).unwrap_err(),
        StatelessTransactionValidatorError::CalldataTooLong {
            calldata_length: 2,
            max_calldata_length: 1
        }
    );
}

#[rstest]
fn test_signature_too_long(
    #[values(TransactionType::Declare, TransactionType::DeployAccount, TransactionType::Invoke)]
    tx_type: TransactionType,
) {
    let tx_validator =
        StatelessTransactionValidator { config: DEFAULT_VALIDATOR_CONFIG_FOR_TESTING };
    let tx = external_tx_for_testing(
        tx_type,
        non_zero_resource_bounds_mapping(),
        calldata![],
        TransactionSignature(vec![StarkFelt::from_u128(1), StarkFelt::from_u128(2)]),
    );

    assert_eq!(
        tx_validator.validate(&tx).unwrap_err(),
        StatelessTransactionValidatorError::SignatureTooLong {
            signature_length: 2,
            max_signature_length: 1
        }
    );
}
