use std::sync::OnceLock;
use std::vec;

use assert_matches::assert_matches;
use mempool_test_utils::declare_tx_args;
use mempool_test_utils::starknet_api_test_utils::{
    rpc_declare_tx,
    rpc_tx_for_testing,
    zero_resource_bounds_mapping,
    TransactionType,
    NON_EMPTY_RESOURCE_BOUNDS,
};
use rstest::rstest;
use starknet_api::core::EntryPointSelector;
use starknet_api::rpc_transaction::{ContractClass, EntryPointByType};
use starknet_api::state::EntryPoint;
use starknet_api::transaction::{
    AllResourceBounds,
    Calldata,
    Resource,
    ResourceBounds,
    TransactionSignature,
};
use starknet_api::{calldata, felt};
use starknet_types_core::felt::Felt;

use crate::compiler_version::{VersionId, VersionIdError};
use crate::config::StatelessTransactionValidatorConfig;
use crate::errors::StatelessTransactionValidatorResult;
use crate::stateless_transaction_validator::{
    StatelessTransactionValidator,
    StatelessTransactionValidatorError,
};
use crate::test_utils::create_sierra_program;

fn min_sierra_version() -> &'static VersionId {
    static MIN_SIERRA_VERSION: OnceLock<VersionId> = OnceLock::new();
    MIN_SIERRA_VERSION.get_or_init(|| VersionId::new(1, 1, 0))
}
fn max_sierra_version() -> &'static VersionId {
    static MAX_SIERRA_VERSION: OnceLock<VersionId> = OnceLock::new();
    MAX_SIERRA_VERSION.get_or_init(|| VersionId::new(1, 5, usize::MAX))
}
fn default_validator_config_for_testing() -> &'static StatelessTransactionValidatorConfig {
    static DEFAULT_VALIDATOR_CONFIG_FOR_TESTING: OnceLock<StatelessTransactionValidatorConfig> =
        OnceLock::new();
    DEFAULT_VALIDATOR_CONFIG_FOR_TESTING.get_or_init(|| StatelessTransactionValidatorConfig {
        validate_non_zero_l1_gas_fee: false,
        validate_non_zero_l2_gas_fee: false,
        max_calldata_length: 1,
        max_signature_length: 1,
        max_contract_class_object_size: 100000,
        min_sierra_version: *min_sierra_version(),
        max_sierra_version: *max_sierra_version(),
    })
}

#[rstest]
#[case::ignore_resource_bounds(
    StatelessTransactionValidatorConfig{
        validate_non_zero_l1_gas_fee: false,
        validate_non_zero_l2_gas_fee: false,
        ..default_validator_config_for_testing().clone()
    },
    zero_resource_bounds_mapping(),
    calldata![],
    TransactionSignature::default()
)]
#[case::valid_l1_gas(
    StatelessTransactionValidatorConfig{
        validate_non_zero_l1_gas_fee: true,
        validate_non_zero_l2_gas_fee: false,
        ..default_validator_config_for_testing().clone()
    },
    AllResourceBounds {
        l1_gas: NON_EMPTY_RESOURCE_BOUNDS,
        ..Default::default()
    },
    calldata![],
    TransactionSignature::default()
)]
#[case::valid_l2_gas(
    StatelessTransactionValidatorConfig{
        validate_non_zero_l1_gas_fee: false,
        validate_non_zero_l2_gas_fee: true,
        ..default_validator_config_for_testing().clone()
    },
    AllResourceBounds {
        l2_gas: NON_EMPTY_RESOURCE_BOUNDS,
        ..Default::default()
    },
    calldata![],
    TransactionSignature::default()
)]
#[case::valid_l1_and_l2_gas(
    StatelessTransactionValidatorConfig{
        validate_non_zero_l1_gas_fee: true,
        validate_non_zero_l2_gas_fee: true,
        ..default_validator_config_for_testing().clone()
    },
    AllResourceBounds {
        l1_gas: NON_EMPTY_RESOURCE_BOUNDS,
        l2_gas: NON_EMPTY_RESOURCE_BOUNDS,
        ..Default::default()
    },
    calldata![],
    TransactionSignature::default()
)]
#[case::non_empty_valid_calldata(
    default_validator_config_for_testing().clone(),
    zero_resource_bounds_mapping(),
    calldata![Felt::ONE],
    TransactionSignature::default()
)]
#[case::non_empty_valid_signature(
    default_validator_config_for_testing().clone(),
    zero_resource_bounds_mapping(),
    calldata![],
    TransactionSignature(vec![Felt::ONE])
)]
#[case::valid_tx(
    default_validator_config_for_testing().clone(),
    zero_resource_bounds_mapping(),
    calldata![],
    TransactionSignature::default()
)]
fn test_positive_flow(
    #[case] config: StatelessTransactionValidatorConfig,
    #[case] resource_bounds: AllResourceBounds,
    #[case] tx_calldata: Calldata,
    #[case] signature: TransactionSignature,
    #[values(TransactionType::Declare, TransactionType::DeployAccount, TransactionType::Invoke)]
    tx_type: TransactionType,
) {
    let tx_validator = StatelessTransactionValidator { config };
    let tx = rpc_tx_for_testing(tx_type, resource_bounds, tx_calldata, signature);

    assert_matches!(tx_validator.validate(&tx), Ok(()));
}

#[rstest]
#[case::zero_l1_gas_resource_bounds(
    StatelessTransactionValidatorConfig{
        validate_non_zero_l1_gas_fee: true,
        validate_non_zero_l2_gas_fee: false,
        ..default_validator_config_for_testing().clone()
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
        ..default_validator_config_for_testing().clone()
    },
    AllResourceBounds {
        l1_gas: NON_EMPTY_RESOURCE_BOUNDS,
        ..Default::default()
    },
    StatelessTransactionValidatorError::ZeroResourceBounds{
        resource: Resource::L2Gas, resource_bounds: ResourceBounds::default()
    }
)]
fn test_invalid_resource_bounds(
    #[case] config: StatelessTransactionValidatorConfig,
    #[case] resource_bounds: AllResourceBounds,
    #[case] expected_error: StatelessTransactionValidatorError,
    #[values(TransactionType::Declare, TransactionType::DeployAccount, TransactionType::Invoke)]
    tx_type: TransactionType,
) {
    let tx_validator = StatelessTransactionValidator { config };
    let tx =
        rpc_tx_for_testing(tx_type, resource_bounds, calldata![], TransactionSignature::default());

    assert_eq!(tx_validator.validate(&tx).unwrap_err(), expected_error);
}

#[rstest]
fn test_calldata_too_long(
    #[values(TransactionType::DeployAccount, TransactionType::Invoke)] tx_type: TransactionType,
) {
    let tx_validator =
        StatelessTransactionValidator { config: default_validator_config_for_testing().clone() };
    let tx = rpc_tx_for_testing(
        tx_type,
        zero_resource_bounds_mapping(),
        calldata![Felt::ONE, Felt::TWO],
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
        StatelessTransactionValidator { config: default_validator_config_for_testing().clone() };
    let tx = rpc_tx_for_testing(
        tx_type,
        zero_resource_bounds_mapping(),
        calldata![],
        TransactionSignature(vec![Felt::ONE, Felt::TWO]),
    );

    assert_eq!(
        tx_validator.validate(&tx).unwrap_err(),
        StatelessTransactionValidatorError::SignatureTooLong {
            signature_length: 2,
            max_signature_length: 1
        }
    );
}

#[rstest]
#[case::sierra_program_length_zero(
    vec![],
    StatelessTransactionValidatorError::InvalidSierraVersion (
        VersionIdError::InvalidVersion {
            message: "Failed to retrieve version from the program: insufficient length. Expected \
                     at least 6 felts (got 0).".into()
        }
    )
)]
#[case::sierra_program_length_one(
    vec![felt!(1_u128)],
    StatelessTransactionValidatorError::InvalidSierraVersion (
        VersionIdError::InvalidVersion {
            message: "Failed to retrieve version from the program: insufficient length. Expected \
                     at least 6 felts (got 1).".into()
        }
    )
)]
#[case::sierra_program_length_three(
    vec![felt!(1_u128), felt!(3_u128), felt!(0_u128)],
    StatelessTransactionValidatorError::InvalidSierraVersion (
        VersionIdError::InvalidVersion {
            message: "Failed to retrieve version from the program: insufficient length. Expected \
                     at least 6 felts (got 3).".into()
        }
    )
)]
#[case::sierra_program_length_four(
    vec![felt!(1_u128), felt!(3_u128), felt!(0_u128), felt!(0_u128)],
    StatelessTransactionValidatorError::InvalidSierraVersion (
        VersionIdError::InvalidVersion {
            message: "Failed to retrieve version from the program: insufficient length. Expected \
                     at least 6 felts (got 4).".into()
        }
    )
)]
#[case::invalid_character_in_sierra_version(
    vec![
            felt!(1_u128),
            felt!(3_u128),
            felt!(0x10000000000000000_u128), // Does not fit into a usize.
            felt!(0_u128),
            felt!(0_u128),
            felt!(0_u128),
    ],
    StatelessTransactionValidatorError::InvalidSierraVersion (
            VersionIdError::InvalidVersion {
                message: "Error extracting version ID from Sierra program: \
                         Invalid input for deserialization.".into()
            }
        )
    )
]
#[case::sierra_version_too_low(
    create_sierra_program(&VersionId::new(0,3,0)),
    StatelessTransactionValidatorError::UnsupportedSierraVersion {
            version: VersionId::new(0,3,0),
            min_version: *min_sierra_version(),
            max_version: *max_sierra_version(),
    })
]
#[case::sierra_version_too_high(
    create_sierra_program(&VersionId::new(1,6,0)),
    StatelessTransactionValidatorError::UnsupportedSierraVersion {
            version: VersionId::new(1,6,0),
            min_version: *min_sierra_version(),
            max_version: *max_sierra_version(),
    })
]
fn test_declare_sierra_version_failure(
    #[case] sierra_program: Vec<Felt>,
    #[case] expected_error: StatelessTransactionValidatorError,
) {
    let tx_validator =
        StatelessTransactionValidator { config: default_validator_config_for_testing().clone() };

    let contract_class = ContractClass { sierra_program, ..Default::default() };
    let tx = rpc_declare_tx(declare_tx_args!(contract_class));

    assert_eq!(tx_validator.validate(&tx).unwrap_err(), expected_error);
}

#[rstest]
#[case::min_sierra_version(create_sierra_program(min_sierra_version()))]
#[case::valid_sierra_version(create_sierra_program(&VersionId::new( 1, 3, 0 )))]
#[case::max_sierra_version_patch_zero(create_sierra_program(
    &VersionId::new( max_sierra_version().0.major, max_sierra_version().0.minor, 0)
))]
#[case::max_sierra_version_patch_non_trivial(create_sierra_program(
    &VersionId::new(max_sierra_version().0.major, max_sierra_version().0.minor, 1)
))]
#[case::max_sierra_version(create_sierra_program(max_sierra_version()))]
fn test_declare_sierra_version_sucsses(#[case] sierra_program: Vec<Felt>) {
    let tx_validator =
        StatelessTransactionValidator { config: default_validator_config_for_testing().clone() };

    let contract_class = ContractClass { sierra_program, ..Default::default() };
    let tx = rpc_declare_tx(declare_tx_args!(contract_class));

    assert_matches!(tx_validator.validate(&tx), Ok(()));
}

#[test]
fn test_declare_contract_class_size_too_long() {
    let config_max_contract_class_object_size = 100; // Some arbitrary value, which will fail the test.
    let tx_validator = StatelessTransactionValidator {
        config: StatelessTransactionValidatorConfig {
            max_contract_class_object_size: config_max_contract_class_object_size,
            ..default_validator_config_for_testing().clone()
        },
    };
    let contract_class = ContractClass {
        sierra_program: create_sierra_program(min_sierra_version()),
        ..Default::default()
    };
    let contract_class_length = serde_json::to_string(&contract_class).unwrap().len();
    let tx = rpc_declare_tx(declare_tx_args!(contract_class));

    assert_matches!(
        tx_validator.validate(&tx).unwrap_err(),
        StatelessTransactionValidatorError::ContractClassObjectSizeTooLarge {
            contract_class_object_size, max_contract_class_object_size
        } if (
            contract_class_object_size, max_contract_class_object_size
        ) == (contract_class_length, config_max_contract_class_object_size)
    )
}

#[rstest]
#[case::valid(
    vec![
        EntryPoint { selector: EntryPointSelector(felt!(1_u128)), ..Default::default() },
        EntryPoint { selector: EntryPointSelector(felt!(2_u128)), ..Default::default() }
    ],
    Ok(())
)]
#[case::no_entry_points(
    vec![],
    Ok(())
)]
#[case::single_entry_point(
    vec![
        EntryPoint { selector: EntryPointSelector(felt!(1_u128)), ..Default::default() }
    ],
    Ok(())
)]
#[case::not_sorted(
    vec![
        EntryPoint { selector: EntryPointSelector(felt!(2_u128)), ..Default::default() },
        EntryPoint { selector: EntryPointSelector(felt!(1_u128)), ..Default::default() },
    ],
    Err(StatelessTransactionValidatorError::EntryPointsNotUniquelySorted)
)]
#[case::not_unique(
    vec![
        EntryPoint { selector: EntryPointSelector(felt!(1_u128)), ..Default::default() },
        EntryPoint { selector: EntryPointSelector(felt!(1_u128)), ..Default::default() },
    ],
    Err(StatelessTransactionValidatorError::EntryPointsNotUniquelySorted)
)]
#[case::many_entry_points(
    vec![
        EntryPoint { selector: EntryPointSelector(felt!(1_u128)), ..Default::default() },
        EntryPoint { selector: EntryPointSelector(felt!(2_u128)), ..Default::default() },
        EntryPoint { selector: EntryPointSelector(felt!(1_u128)), ..Default::default() },
    ],
    Err(StatelessTransactionValidatorError::EntryPointsNotUniquelySorted)
)]
fn test_declare_entry_points_not_sorted_by_selector(
    #[case] entry_points: Vec<EntryPoint>,
    #[case] expected: StatelessTransactionValidatorResult<()>,
) {
    let tx_validator =
        StatelessTransactionValidator { config: default_validator_config_for_testing().clone() };

    let contract_class = ContractClass {
        sierra_program: create_sierra_program(min_sierra_version()),
        entry_points_by_type: EntryPointByType {
            constructor: entry_points.clone(),
            external: vec![],
            l1handler: vec![],
        },
        ..Default::default()
    };
    let tx = rpc_declare_tx(declare_tx_args!(contract_class));

    assert_eq!(tx_validator.validate(&tx), expected);

    let contract_class = ContractClass {
        sierra_program: create_sierra_program(min_sierra_version()),
        entry_points_by_type: EntryPointByType {
            constructor: vec![],
            external: entry_points.clone(),
            l1handler: vec![],
        },
        ..Default::default()
    };
    let tx = rpc_declare_tx(declare_tx_args!(contract_class));

    assert_eq!(tx_validator.validate(&tx), expected);

    let contract_class = ContractClass {
        sierra_program: create_sierra_program(min_sierra_version()),
        entry_points_by_type: EntryPointByType {
            constructor: vec![],
            external: vec![],
            l1handler: entry_points,
        },
        ..Default::default()
    };
    let tx = rpc_declare_tx(declare_tx_args!(contract_class));

    assert_eq!(tx_validator.validate(&tx), expected);
}
