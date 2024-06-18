use assert_matches::assert_matches;
use rstest::rstest;
use starknet_api::hash::StarkFelt;
use starknet_api::rpc_transaction::{ContractClass, ResourceBoundsMapping};
use starknet_api::transaction::{Calldata, Resource, ResourceBounds, TransactionSignature};
use starknet_api::{calldata, stark_felt};

use crate::config::StatelessTransactionValidatorConfig;
use crate::declare_tx_args;
use crate::starknet_api_test_utils::{
    create_resource_bounds_mapping, external_declare_tx, external_tx_for_testing,
    zero_resource_bounds_mapping, TransactionType, NON_EMPTY_RESOURCE_BOUNDS,
};
use crate::stateless_transaction_validator::{
    StatelessTransactionValidator, StatelessTransactionValidatorError,
};

const DEFAULT_VALIDATOR_CONFIG_FOR_TESTING: StatelessTransactionValidatorConfig =
    StatelessTransactionValidatorConfig {
        validate_non_zero_l1_gas_fee: false,
        validate_non_zero_l2_gas_fee: false,
        max_calldata_length: 1,
        max_signature_length: 1,
        max_bytecode_size: 10000,
        max_raw_class_size: 100000,
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
#[case::valid_l1_gas(
    StatelessTransactionValidatorConfig{
        validate_non_zero_l1_gas_fee: true,
        validate_non_zero_l2_gas_fee: false,
        ..DEFAULT_VALIDATOR_CONFIG_FOR_TESTING
    },
    create_resource_bounds_mapping(NON_EMPTY_RESOURCE_BOUNDS, ResourceBounds::default()),
    calldata![],
    TransactionSignature::default()
)]
#[case::valid_l2_gas(
    StatelessTransactionValidatorConfig{
        validate_non_zero_l1_gas_fee: false,
        validate_non_zero_l2_gas_fee: true,
        ..DEFAULT_VALIDATOR_CONFIG_FOR_TESTING
    },
    create_resource_bounds_mapping(ResourceBounds::default(), NON_EMPTY_RESOURCE_BOUNDS),
    calldata![],
    TransactionSignature::default()
)]
#[case::valid_l1_and_l2_gas(
    StatelessTransactionValidatorConfig{
        validate_non_zero_l1_gas_fee: true,
        validate_non_zero_l2_gas_fee: true,
        ..DEFAULT_VALIDATOR_CONFIG_FOR_TESTING
    },
    create_resource_bounds_mapping(NON_EMPTY_RESOURCE_BOUNDS, NON_EMPTY_RESOURCE_BOUNDS),
    calldata![],
    TransactionSignature::default()
)]
#[case::non_empty_valid_calldata(
    DEFAULT_VALIDATOR_CONFIG_FOR_TESTING,
    zero_resource_bounds_mapping(),
    calldata![StarkFelt::from_u128(1)],
    TransactionSignature::default()
)]
#[case::non_empty_valid_signature(
    DEFAULT_VALIDATOR_CONFIG_FOR_TESTING,
    zero_resource_bounds_mapping(),
    calldata![],
    TransactionSignature(vec![StarkFelt::from_u128(1)])
)]
#[case::valid_tx(
    DEFAULT_VALIDATOR_CONFIG_FOR_TESTING,
    zero_resource_bounds_mapping(),
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
    StatelessTransactionValidatorConfig{
        validate_non_zero_l1_gas_fee: true,
        validate_non_zero_l2_gas_fee: false,
        ..DEFAULT_VALIDATOR_CONFIG_FOR_TESTING
    },
    zero_resource_bounds_mapping(),
    StatelessTransactionValidatorError::ZeroResourceBounds{
        resource: Resource::L1Gas, resource_bounds: ResourceBounds::default()
    }
)]
#[case::zero_l2_gas_resource_bounds(
    StatelessTransactionValidatorConfig{
        validate_non_zero_l1_gas_fee: false,
        validate_non_zero_l2_gas_fee: true,
        ..DEFAULT_VALIDATOR_CONFIG_FOR_TESTING
    },
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
        zero_resource_bounds_mapping(),
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
        zero_resource_bounds_mapping(),
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

#[test]
fn test_declare_bytecode_size_too_long() {
    let config_max_bytecode_size = 10;
    let tx_validator = StatelessTransactionValidator {
        config: StatelessTransactionValidatorConfig {
            max_bytecode_size: config_max_bytecode_size,
            ..DEFAULT_VALIDATOR_CONFIG_FOR_TESTING
        },
    };
    let sierra_program_length = config_max_bytecode_size + 1;
    let sierra_program = vec![stark_felt!(1_u128); sierra_program_length];
    let contract_class = ContractClass { sierra_program, ..Default::default() };
    let tx = external_declare_tx(declare_tx_args!(contract_class));

    assert_matches!(
        tx_validator.validate(&tx).unwrap_err(),
        StatelessTransactionValidatorError::BytecodeSizeTooLarge {
                bytecode_size,
                max_bytecode_size
            } if (
                bytecode_size, max_bytecode_size
            ) == (sierra_program_length, config_max_bytecode_size)
    )
}

#[test]
fn test_declare_contract_class_size_too_long() {
    let config_max_raw_class_size = 100;
    let tx_validator = StatelessTransactionValidator {
        config: StatelessTransactionValidatorConfig {
            max_raw_class_size: config_max_raw_class_size,
            ..DEFAULT_VALIDATOR_CONFIG_FOR_TESTING
        },
    };
    let contract_class =
        ContractClass { sierra_program: vec![stark_felt!(1_u128); 3], ..Default::default() };
    let contract_class_length = serde_json::to_string(&contract_class).unwrap().len();
    let tx = external_declare_tx(declare_tx_args!(contract_class));

    assert_matches!(
        tx_validator.validate(&tx).unwrap_err(),
        StatelessTransactionValidatorError::ContractClassObjectSizeTooLarge {
            contract_class_object_size, max_contract_class_object_size
        } if (
            contract_class_object_size, max_contract_class_object_size
        ) == (contract_class_length, config_max_raw_class_size)
    )
}
