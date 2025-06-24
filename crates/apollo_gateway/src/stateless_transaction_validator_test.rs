use std::sync::LazyLock;
use std::vec;

use assert_matches::assert_matches;
use rstest::rstest;
use starknet_api::block::GasPrice;
use starknet_api::core::{EntryPointSelector, L2_ADDRESS_UPPER_BOUND};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::rpc_transaction::EntryPointByType;
use starknet_api::state::{EntryPoint, SierraContractClass};
use starknet_api::test_utils::declare::rpc_declare_tx;
use starknet_api::transaction::fields::{
    AccountDeploymentData,
    AllResourceBounds,
    PaymasterData,
    ResourceBounds,
    TransactionSignature,
};
use starknet_api::{calldata, contract_address, declare_tx_args, felt, StarknetApiError};
use starknet_types_core::felt::Felt;

use crate::compiler_version::{VersionId, VersionIdError};
use crate::config::StatelessTransactionValidatorConfig;
use crate::errors::StatelessTransactionValidatorResult;
use crate::stateless_transaction_validator::{
    StatelessTransactionValidator,
    StatelessTransactionValidatorError,
};
use crate::test_utils::{
    create_sierra_program,
    rpc_tx_for_testing,
    RpcTransactionArgs,
    TransactionType,
    NON_EMPTY_RESOURCE_BOUNDS,
};

static MIN_SIERRA_VERSION: LazyLock<VersionId> = LazyLock::new(|| VersionId::new(1, 1, 0));
static MAX_SIERRA_VERSION: LazyLock<VersionId> = LazyLock::new(|| VersionId::new(1, 5, usize::MAX));

static DEFAULT_VALIDATOR_CONFIG_FOR_TESTING: LazyLock<StatelessTransactionValidatorConfig> =
    LazyLock::new(|| StatelessTransactionValidatorConfig {
        validate_non_zero_resource_bounds: false,
        min_gas_price: 0,
        max_calldata_length: 1,
        max_signature_length: 1,
        max_contract_bytecode_size: 100000,
        max_contract_class_object_size: 100000,
        min_sierra_version: *MIN_SIERRA_VERSION,
        max_sierra_version: *MAX_SIERRA_VERSION,
    });

#[rstest]
#[case::valid_l1_gas(
    StatelessTransactionValidatorConfig {
        validate_non_zero_resource_bounds: true,
        ..*DEFAULT_VALIDATOR_CONFIG_FOR_TESTING
    },
    RpcTransactionArgs {
        resource_bounds: AllResourceBounds {
            l1_gas: NON_EMPTY_RESOURCE_BOUNDS,
            ..Default::default()
        },
        ..Default::default()
    }
)]
#[case::valid_l2_gas(
    StatelessTransactionValidatorConfig {
        validate_non_zero_resource_bounds: true,
        ..*DEFAULT_VALIDATOR_CONFIG_FOR_TESTING
    },
    RpcTransactionArgs {
        resource_bounds: AllResourceBounds {
            l2_gas: NON_EMPTY_RESOURCE_BOUNDS,
            ..Default::default()
        },
        ..Default::default()
    }
)]
#[case::valid_l1_and_l2_gas(
    StatelessTransactionValidatorConfig {
        validate_non_zero_resource_bounds: true,
        ..*DEFAULT_VALIDATOR_CONFIG_FOR_TESTING
    },
    RpcTransactionArgs {
        resource_bounds: AllResourceBounds {
            l1_gas: NON_EMPTY_RESOURCE_BOUNDS,
            l2_gas: NON_EMPTY_RESOURCE_BOUNDS,
            ..Default::default()
        },
        ..Default::default()
    }
)]
#[case::valid_l1_data_gas(
    StatelessTransactionValidatorConfig {
        validate_non_zero_resource_bounds: true,
        ..*DEFAULT_VALIDATOR_CONFIG_FOR_TESTING
    },
    RpcTransactionArgs {
        resource_bounds: AllResourceBounds {
            l1_data_gas: NON_EMPTY_RESOURCE_BOUNDS,
            ..Default::default()
        },
        ..Default::default()
    }
)]
#[case::non_empty_valid_calldata(
    DEFAULT_VALIDATOR_CONFIG_FOR_TESTING.clone(),
    RpcTransactionArgs { calldata: calldata![Felt::ONE], ..Default::default()}
)]
#[case::non_empty_valid_signature(
    DEFAULT_VALIDATOR_CONFIG_FOR_TESTING.clone(),
    RpcTransactionArgs { signature: TransactionSignature(vec![Felt::ONE].into()), ..Default::default()}
)]
#[case::valid_tx(DEFAULT_VALIDATOR_CONFIG_FOR_TESTING.clone(), RpcTransactionArgs::default())]
fn test_positive_flow(
    #[case] config: StatelessTransactionValidatorConfig,
    #[case] rpc_tx_args: RpcTransactionArgs,
    #[values(TransactionType::Declare, TransactionType::DeployAccount, TransactionType::Invoke)]
    tx_type: TransactionType,
) {
    let tx_validator = StatelessTransactionValidator { config };

    let tx = rpc_tx_for_testing(tx_type, rpc_tx_args);

    assert_matches!(tx_validator.validate(&tx), Ok(()));
}

#[rstest]
#[case::zero_resource_bounds(
    RpcTransactionArgs {
        resource_bounds: AllResourceBounds::default(),
        ..Default::default()
    },
    StatelessTransactionValidatorError::ZeroResourceBounds {
        resource_bounds: AllResourceBounds::default()
    },
)]
#[case::max_l2_gas_price_below_min(
    RpcTransactionArgs {
        resource_bounds: AllResourceBounds {
            l2_gas: ResourceBounds {
                max_price_per_unit: GasPrice(99_999_999_u128),
                ..NON_EMPTY_RESOURCE_BOUNDS
            },
            ..Default::default()
        },
        ..Default::default()
    },
    StatelessTransactionValidatorError::MaxGasPriceTooLow {
        gas_price: GasPrice(99_999_999_u128),
        min_gas_price: 100_000_000_u128
    },
)]
fn test_invalid_resource_bounds(
    #[case] rpc_tx_args: RpcTransactionArgs,
    #[case] expected_error: StatelessTransactionValidatorError,
    #[values(TransactionType::Declare, TransactionType::DeployAccount, TransactionType::Invoke)]
    tx_type: TransactionType,
) {
    let config = StatelessTransactionValidatorConfig {
        validate_non_zero_resource_bounds: true,
        min_gas_price: 100_000_000_u128,
        ..*DEFAULT_VALIDATOR_CONFIG_FOR_TESTING
    };
    let tx_validator = StatelessTransactionValidator { config };

    let tx = rpc_tx_for_testing(tx_type, rpc_tx_args);

    assert_eq!(tx_validator.validate(&tx).unwrap_err(), expected_error);
}

#[rstest]
#[case::calldata_too_long(
    RpcTransactionArgs { calldata: calldata![Felt::ONE, Felt::TWO], ..Default::default() },
    StatelessTransactionValidatorError::CalldataTooLong {
        calldata_length: 2,
        max_calldata_length: 1
    },
    vec![TransactionType::DeployAccount, TransactionType::Invoke],
)]
#[case::signature_too_long(
    RpcTransactionArgs {
        signature: TransactionSignature(vec![Felt::ONE, Felt::TWO].into()),
        ..Default::default()
    },
    StatelessTransactionValidatorError::SignatureTooLong {
        signature_length: 2,
        max_signature_length: 1
    },
    vec![TransactionType::Declare, TransactionType::DeployAccount, TransactionType::Invoke],
)]
#[case::nonce_data_availability_mode(
    RpcTransactionArgs {
        nonce_data_availability_mode: DataAvailabilityMode::L2,
        ..Default::default()
    },
    StatelessTransactionValidatorError::InvalidDataAvailabilityMode {
        field_name: "nonce".to_string()
    },
    vec![TransactionType::Declare, TransactionType::DeployAccount, TransactionType::Invoke],
)]
#[case::fee_data_availability_mode(
    RpcTransactionArgs {
        fee_data_availability_mode: DataAvailabilityMode::L2,
        ..Default::default()
    },
    StatelessTransactionValidatorError::InvalidDataAvailabilityMode {
        field_name: "fee".to_string()
    },
    vec![TransactionType::Declare, TransactionType::DeployAccount, TransactionType::Invoke],
)]
#[case::non_empty_account_deployment_data(
    RpcTransactionArgs {
        account_deployment_data: AccountDeploymentData(vec![felt!(1_u128)]),
        ..Default::default()
    },
    StatelessTransactionValidatorError::NonEmptyField {
        field_name: "account_deployment_data".to_string()
    },
    vec![TransactionType::Declare, TransactionType::Invoke],
)]
#[case::non_empty_paymaster_data(
    RpcTransactionArgs {
        paymaster_data: PaymasterData(vec![felt!(1_u128)]),
        ..Default::default()
    },
    StatelessTransactionValidatorError::NonEmptyField {
        field_name: "paymaster_data".to_string()
    },
    vec![TransactionType::Declare, TransactionType::Invoke],
)]
#[case::contract_address_1(
    RpcTransactionArgs {
        sender_address: contract_address!(1_u32),
        ..Default::default()
    },
    StatelessTransactionValidatorError::StarknetApiError(StarknetApiError::OutOfRange {
        string: format!("[0x2, {})", Felt::from(*L2_ADDRESS_UPPER_BOUND))
    }),
    vec![TransactionType::Declare, TransactionType::Invoke],
)]
#[case::contract_address_upper_bound(
    RpcTransactionArgs {
        sender_address: contract_address!("7FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF00"),
        ..Default::default()
    },
    StatelessTransactionValidatorError::StarknetApiError(StarknetApiError::OutOfRange {
        string: format!("[0x2, {})", Felt::from(*L2_ADDRESS_UPPER_BOUND))
    }),
    vec![TransactionType::Declare, TransactionType::Invoke],
)]
fn test_invalid_tx(
    #[case] rpc_tx_args: RpcTransactionArgs,
    #[case] expected_error: StatelessTransactionValidatorError,
    #[case] tx_types: Vec<TransactionType>,
) {
    let tx_validator =
        StatelessTransactionValidator { config: DEFAULT_VALIDATOR_CONFIG_FOR_TESTING.clone() };
    for tx_type in tx_types {
        let tx = rpc_tx_for_testing(tx_type, rpc_tx_args.clone());

        assert_eq!(tx_validator.validate(&tx).unwrap_err(), expected_error);
    }
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
            min_version: *MIN_SIERRA_VERSION,
            max_version: *MAX_SIERRA_VERSION,
    })
]
#[case::sierra_version_too_high(
    create_sierra_program(&VersionId::new(1,6,0)),
    StatelessTransactionValidatorError::UnsupportedSierraVersion {
            version: VersionId::new(1,6,0),
            min_version: *MIN_SIERRA_VERSION,
            max_version: *MAX_SIERRA_VERSION,
    })
]
fn test_declare_sierra_version_failure(
    #[case] sierra_program: Vec<Felt>,
    #[case] expected_error: StatelessTransactionValidatorError,
) {
    let tx_validator =
        StatelessTransactionValidator { config: DEFAULT_VALIDATOR_CONFIG_FOR_TESTING.clone() };

    let contract_class = SierraContractClass { sierra_program, ..Default::default() };
    let tx = rpc_declare_tx(declare_tx_args!(), contract_class);

    assert_eq!(tx_validator.validate(&tx).unwrap_err(), expected_error);
}

#[rstest]
#[case::min_sierra_version(create_sierra_program(&MIN_SIERRA_VERSION))]
#[case::valid_sierra_version(create_sierra_program(&VersionId::new( 1, 3, 0 )))]
#[case::max_sierra_version_patch_zero(create_sierra_program(
    &VersionId::new( MAX_SIERRA_VERSION.0.major, MAX_SIERRA_VERSION.0.minor, 0)
))]
#[case::max_sierra_version_patch_non_trivial(create_sierra_program(
    &VersionId::new(MAX_SIERRA_VERSION.0.major, MAX_SIERRA_VERSION.0.minor, 1)
))]
#[case::max_sierra_version(create_sierra_program(&MAX_SIERRA_VERSION))]
fn test_declare_sierra_version_sucsses(#[case] sierra_program: Vec<Felt>) {
    let tx_validator =
        StatelessTransactionValidator { config: DEFAULT_VALIDATOR_CONFIG_FOR_TESTING.clone() };

    let contract_class = SierraContractClass { sierra_program, ..Default::default() };
    let tx = rpc_declare_tx(declare_tx_args!(), contract_class);

    assert_matches!(tx_validator.validate(&tx), Ok(()));
}

#[test]
fn test_declare_contract_class_size_too_long() {
    let config_max_contract_class_object_size = 100; // Some arbitrary value, which will fail the test.
    let tx_validator = StatelessTransactionValidator {
        config: StatelessTransactionValidatorConfig {
            max_contract_class_object_size: config_max_contract_class_object_size,
            ..*DEFAULT_VALIDATOR_CONFIG_FOR_TESTING
        },
    };
    let contract_class = SierraContractClass {
        sierra_program: create_sierra_program(&MIN_SIERRA_VERSION),
        ..Default::default()
    };
    let contract_class_length = serde_json::to_string(&contract_class).unwrap().len();
    let tx = rpc_declare_tx(declare_tx_args!(), contract_class);

    assert_matches!(
        tx_validator.validate(&tx).unwrap_err(),
        StatelessTransactionValidatorError::ContractClassObjectSizeTooLarge {
            contract_class_object_size, max_contract_class_object_size
        } if (
            contract_class_object_size, max_contract_class_object_size
        ) == (contract_class_length, config_max_contract_class_object_size)
    )
}

#[test]
fn test_declare_contract_bytecode_size_too_long() {
    let sierra_program = create_sierra_program(&MIN_SIERRA_VERSION);
    assert!(sierra_program.len() > 1);
    let tx_validator = StatelessTransactionValidator {
        config: StatelessTransactionValidatorConfig {
            max_contract_bytecode_size: sierra_program.len() - 1,
            ..*DEFAULT_VALIDATOR_CONFIG_FOR_TESTING
        },
    };

    let tx = rpc_declare_tx(
        declare_tx_args!(),
        SierraContractClass { sierra_program, ..Default::default() },
    );

    assert_matches!(
        tx_validator.validate(&tx),
        Err(StatelessTransactionValidatorError::ContractBytecodeSizeTooLarge { .. })
    );
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
        StatelessTransactionValidator { config: DEFAULT_VALIDATOR_CONFIG_FOR_TESTING.clone() };

    let contract_class = SierraContractClass {
        sierra_program: create_sierra_program(&MIN_SIERRA_VERSION),
        entry_points_by_type: EntryPointByType {
            constructor: entry_points.clone(),
            external: vec![],
            l1handler: vec![],
        },
        ..Default::default()
    };
    let tx = rpc_declare_tx(declare_tx_args!(), contract_class);

    assert_eq!(tx_validator.validate(&tx), expected);

    let contract_class = SierraContractClass {
        sierra_program: create_sierra_program(&MIN_SIERRA_VERSION),
        entry_points_by_type: EntryPointByType {
            constructor: vec![],
            external: entry_points.clone(),
            l1handler: vec![],
        },
        ..Default::default()
    };
    let tx = rpc_declare_tx(declare_tx_args!(), contract_class);

    assert_eq!(tx_validator.validate(&tx), expected);

    let contract_class = SierraContractClass {
        sierra_program: create_sierra_program(&MIN_SIERRA_VERSION),
        entry_points_by_type: EntryPointByType {
            constructor: vec![],
            external: vec![],
            l1handler: entry_points,
        },
        ..Default::default()
    };
    let tx = rpc_declare_tx(declare_tx_args!(), contract_class);

    assert_eq!(tx_validator.validate(&tx), expected);
}
