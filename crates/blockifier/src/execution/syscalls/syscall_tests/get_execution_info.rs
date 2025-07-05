use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::block::GasPrice;
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::execution_resources::GasAmount;
use starknet_api::test_utils::{
    CHAIN_ID_FOR_TESTS,
    CURRENT_BLOCK_NUMBER,
    CURRENT_BLOCK_NUMBER_FOR_VALIDATE,
    CURRENT_BLOCK_TIMESTAMP,
    CURRENT_BLOCK_TIMESTAMP_FOR_VALIDATE,
    TEST_SEQUENCER_ADDRESS,
};
use starknet_api::transaction::fields::{
    valid_resource_bounds_as_felts,
    AccountDeploymentData,
    AllResourceBounds,
    Calldata,
    Fee,
    PaymasterData,
    Resource,
    ResourceBounds,
    Tip,
    ValidResourceBounds,
};
use starknet_api::transaction::{TransactionVersion, QUERY_VERSION_BASE};
use starknet_api::{felt, nonce, tx_hash};
use starknet_types_core::felt::Felt;
use test_case::test_case;

use crate::blockifier_versioned_constants::VersionedConstants;
use crate::context::ChainInfo;
use crate::execution::common_hints::ExecutionMode;
use crate::execution::entry_point::CallEntryPoint;
use crate::test_utils::contracts::FeatureContractData;
use crate::test_utils::initial_test_state::test_state_ex;
use crate::test_utils::{trivial_external_entry_point_with_address, BALANCE};
use crate::transaction::objects::{
    CommonAccountFields,
    CurrentTransactionInfo,
    DeprecatedTransactionInfo,
    TransactionInfo,
};

#[cfg_attr(
    feature = "cairo_native",
    test_case(
        FeatureContract::SierraExecutionInfoV1Contract(RunnableCairo1::Native),
        ExecutionMode::Validate,
        TransactionVersion::ONE,
        false,
        false,
        false,
        false;
        "Native: Validate execution mode: block info fields should be zeroed. Transaction V1."
    )
)]
#[cfg_attr(
    feature = "cairo_native",
    test_case(
        FeatureContract::SierraExecutionInfoV1Contract(RunnableCairo1::Native),
        ExecutionMode::Execute,
        TransactionVersion::ONE,
        false,
        false,
        false,
        false;
        "Native: Execute execution mode: block info should be as usual. Transaction V1."
    )
)]
#[cfg_attr(
    feature = "cairo_native",
    test_case(
        FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Native)),
        ExecutionMode::Validate,
        TransactionVersion::THREE,
        false,
        false,
        false,
        false;
        "Native: Validate execution mode: block info fields should be zeroed. Transaction V3."
    )
)]
#[cfg_attr(
    feature = "cairo_native",
    test_case(
        FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Native)),
        ExecutionMode::Execute,
        TransactionVersion::THREE,
        false,
        false,
        false,
        false;
        "Native: Execute execution mode: block info should be as usual. Transaction V3."
    )
)]
#[cfg_attr(
    feature = "cairo_native",
    test_case(
    FeatureContract::LegacyTestContract,
    ExecutionMode::Execute,
    TransactionVersion::ONE,
    false,
    false,
    false,
    false;
    "Native: Legacy contract. Execute execution mode: block info should be as usual. Transaction
    V1."
    )
)]
#[cfg_attr(
    feature = "cairo_native",
    test_case(
    FeatureContract::LegacyTestContract,
    ExecutionMode::Execute,
    TransactionVersion::THREE,
    false,
    false,
    false,
    false;
    "Native: Legacy contract. Execute execution mode: block info should be as usual. Transaction
    V3."
    )
)]
#[cfg_attr(
    feature = "cairo_native",
    test_case(
        FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Native)),
        ExecutionMode::Execute,
        TransactionVersion::THREE,
        true,
        false,
        false,
        false;
        "Native: Execute execution mode: block info should be as usual. Transaction V3. Query"
    )
)]
#[cfg_attr(
    feature = "cairo_native",
    test_case(
        FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Native)),
        ExecutionMode::Execute,
        TransactionVersion::THREE,
        false,
        true,
        false,
        false;
        "Native: V1 bound account: execute"
    )
)]
#[cfg_attr(
    feature = "cairo_native",
    test_case(
        FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Native)),
        ExecutionMode::Execute,
        TransactionVersion::THREE,
        true,
        true,
        false,
        false;
        "Native: V1 bound account: query"
    )
)]
#[cfg_attr(
    feature = "cairo_native",
    test_case(
        FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Native)),
        ExecutionMode::Execute,
        TransactionVersion::THREE,
        true,
        false,
        false,
        true;
        "Native: data gas account: query"
    )
)]
#[cfg_attr(
    feature = "cairo_native",
    test_case(
        FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Native)),
        ExecutionMode::Execute,
        TransactionVersion::THREE,
        false,
        false,
        false,
        true;
        "Native: data gas account"
    )
)]
#[test_case(
    FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm)),
    ExecutionMode::Validate,
    TransactionVersion::ONE,
    false,
    false,
    false,
    false;
    "Validate execution mode: block info fields should be zeroed. Transaction V1.")]
#[test_case(
    FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm)),
    ExecutionMode::Execute,
    TransactionVersion::ONE,
    false,
    false,
    false,
    false;
    "Execute execution mode: block info should be as usual. Transaction V1.")]
#[test_case(
    FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm)),
    ExecutionMode::Validate,
    TransactionVersion::THREE,
    false,
    false,
    false,
    false;
    "Validate execution mode: block info fields should be zeroed. Transaction V3.")]
#[test_case(
    FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm)),
    ExecutionMode::Execute,
    TransactionVersion::THREE,
    false,
    false,
    false,
    false;
    "Execute execution mode: block info should be as usual. Transaction V3.")]
#[test_case(
    FeatureContract::LegacyTestContract,
    ExecutionMode::Execute,
    TransactionVersion::ONE,
    false,
    false,
    false,
    false;
    "Legacy contract. Execute execution mode: block info should be as usual. Transaction V1.")]
#[test_case(
    FeatureContract::LegacyTestContract,
    ExecutionMode::Execute,
    TransactionVersion::THREE,
    false,
    false,
    false,
    false;
    "Legacy contract. Execute execution mode: block info should be as usual. Transaction V3.")]
#[test_case(
    FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm)),
    ExecutionMode::Execute,
    TransactionVersion::THREE,
    true,
    false,
    false,
    false;
    "Execute execution mode: block info should be as usual. Transaction V3. Query.")]
#[test_case(
    FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm)),
    ExecutionMode::Execute,
    TransactionVersion::THREE,
    false,
    true,
    false,
    false;
    "V1 bound account: execute")]
#[test_case(
    FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm)),
    ExecutionMode::Execute,
    TransactionVersion::THREE,
    false,
    true,
    true,
    false;
    "V1 bound account: execute, high tip")]
#[test_case(
    FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm)),
    ExecutionMode::Execute,
    TransactionVersion::THREE,
    true,
    true,
    false,
    false;
    "V1 bound account: query")]
#[test_case(
    FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm)),
    ExecutionMode::Execute,
    TransactionVersion::THREE,
    false,
    false,
    false,
    true;
    "Exclude l1 data gas: execute")]
#[test_case(
    FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm)),
    ExecutionMode::Execute,
    TransactionVersion::THREE,
    true,
    false,
    false,
    true;
    "Exclude l1 data gas: query")]
fn test_get_execution_info(
    test_contract: FeatureContract,
    execution_mode: ExecutionMode,
    mut version: TransactionVersion,
    only_query: bool,
    v1_bound_account: bool,
    // Whether the tip is larger than `v1_bound_accounts_max_tip`.
    high_tip: bool,
    exclude_l1_data_gas: bool,
) {
    let mut test_contract_data: FeatureContractData = test_contract.into();
    if v1_bound_account {
        assert!(
            !exclude_l1_data_gas,
            "Unable to set both exclude_l1_data_gas and v1_bound_account."
        );
        let optional_class_hash =
            VersionedConstants::latest_constants().os_constants.v1_bound_accounts_cairo1.first();
        test_contract_data.class_hash =
            *optional_class_hash.expect("No v1 bound accounts found in versioned constants.");
    } else if exclude_l1_data_gas {
        test_contract_data.class_hash =
            *VersionedConstants::latest_constants().os_constants.data_gas_accounts.first().unwrap();
    }
    let state =
        &mut test_state_ex(&ChainInfo::create_for_testing(), BALANCE, &[(test_contract_data, 1)]);
    let expected_block_info = match execution_mode {
        ExecutionMode::Validate => [
            // Rounded block number.
            felt!(CURRENT_BLOCK_NUMBER_FOR_VALIDATE),
            // Rounded timestamp.
            felt!(CURRENT_BLOCK_TIMESTAMP_FOR_VALIDATE),
            Felt::ZERO,
        ],
        ExecutionMode::Execute => [
            felt!(CURRENT_BLOCK_NUMBER),    // Block number.
            felt!(CURRENT_BLOCK_TIMESTAMP), // Block timestamp.
            Felt::from_hex(TEST_SEQUENCER_ADDRESS).unwrap(),
        ],
    };

    let test_contract_address = test_contract.get_instance_address(0);

    // Transaction tip.
    let tip = Tip(VersionedConstants::latest_constants().os_constants.v1_bound_accounts_max_tip.0
        + if high_tip { 1 } else { 0 });
    let expected_tip = if version == TransactionVersion::THREE { tip } else { Tip(0) };

    let expected_unsupported_fields = match test_contract {
        FeatureContract::LegacyTestContract => {
            // Read and parse file content.
            let raw_contract: serde_json::Value =
                serde_json::from_str(&test_contract.get_raw_class()).expect("Error parsing JSON");
            // Verify version.
            if let Some(compiler_version) = raw_contract["compiler_version"].as_str() {
                assert_eq!(compiler_version, "2.1.0");
            } else {
                panic!("'compiler_version' not found or not a valid string in JSON.");
            };
            vec![]
        }
        #[cfg(feature = "cairo_native")]
        FeatureContract::SierraExecutionInfoV1Contract(RunnableCairo1::Native) => {
            vec![]
        }
        _ => {
            vec![
                expected_tip.into(), // Tip.
                Felt::ZERO,          // Paymaster data.
                Felt::ZERO,          // Nonce DA.
                Felt::ZERO,          // Fee DA.
                Felt::ZERO,          // Account data.
            ]
        }
    };

    let mut expected_version = if v1_bound_account && !high_tip { 1.into() } else { version.0 };
    if only_query {
        let simulate_version_base = *QUERY_VERSION_BASE;
        let query_version = simulate_version_base + version.0;
        version = TransactionVersion(query_version);
        expected_version += simulate_version_base;
    }

    let tx_hash = tx_hash!(1991);
    let max_fee = Fee(42);
    let nonce = nonce!(3_u16);
    let sender_address = test_contract_address;

    let resource_bounds =
        ResourceBounds { max_amount: GasAmount(13), max_price_per_unit: GasPrice(61) };
    let all_resource_bounds = ValidResourceBounds::AllResources(AllResourceBounds {
        l1_gas: resource_bounds,
        l2_gas: resource_bounds,
        l1_data_gas: resource_bounds,
    });

    let expected_resource_bounds: Vec<Felt> = match (test_contract, version) {
        (FeatureContract::LegacyTestContract, _) => vec![],
        #[cfg(feature = "cairo_native")]
        (FeatureContract::SierraExecutionInfoV1Contract(RunnableCairo1::Native), _) => vec![],
        (_, version) if version == TransactionVersion::ONE => vec![
            felt!(0_u16), // Length of resource bounds array.
        ],
        (_, _) => {
            vec![felt!(if exclude_l1_data_gas { 2_u8 } else { 3_u8 })] // Length of resource bounds array.
                .into_iter()
                .chain(
                    valid_resource_bounds_as_felts(&all_resource_bounds, exclude_l1_data_gas)
                        .unwrap()
                        .into_iter()
                        .flat_map(|bounds| bounds.flatten()),
                )
                .collect()
        }
    };

    let expected_tx_info: Vec<Felt>;
    let tx_info: TransactionInfo;
    if version == TransactionVersion::ONE {
        expected_tx_info = vec![
            expected_version,                     // Transaction version.
            *sender_address.0.key(),              // Account address.
            felt!(max_fee.0),                     // Max fee.
            Felt::ZERO,                           // Signature.
            tx_hash.0,                            // Transaction hash.
            felt!(&*CHAIN_ID_FOR_TESTS.as_hex()), // Chain ID.
            nonce.0,                              // Nonce.
        ];

        tx_info = TransactionInfo::Deprecated(DeprecatedTransactionInfo {
            common_fields: CommonAccountFields {
                transaction_hash: tx_hash,
                version: TransactionVersion::ONE,
                nonce,
                sender_address,
                only_query,
                ..Default::default()
            },
            max_fee,
        });
    } else {
        expected_tx_info = vec![
            expected_version,                     // Transaction version.
            *sender_address.0.key(),              // Account address.
            Felt::ZERO,                           // Max fee.
            Felt::ZERO,                           // Signature.
            tx_hash.0,                            // Transaction hash.
            felt!(&*CHAIN_ID_FOR_TESTS.as_hex()), // Chain ID.
            nonce.0,                              // Nonce.
        ];

        tx_info = TransactionInfo::Current(CurrentTransactionInfo {
            common_fields: CommonAccountFields {
                transaction_hash: tx_hash,
                version: TransactionVersion::THREE,
                nonce,
                sender_address,
                only_query,
                ..Default::default()
            },
            resource_bounds: all_resource_bounds,
            tip,
            nonce_data_availability_mode: DataAvailabilityMode::L1,
            fee_data_availability_mode: DataAvailabilityMode::L1,
            paymaster_data: PaymasterData::default(),
            account_deployment_data: AccountDeploymentData::default(),
        });
    }

    let entry_point_selector = selector_from_name("test_get_execution_info");
    let expected_call_info = vec![
        felt!(0_u16),                   // Caller address.
        *test_contract_address.0.key(), // Storage address.
        entry_point_selector.0,         // Entry point selector.
    ];
    let entry_point_call = CallEntryPoint {
        entry_point_selector,
        code_address: None,
        calldata: Calldata(
            [
                expected_block_info.to_vec(),
                expected_tx_info,
                expected_resource_bounds.into_iter().chain(expected_unsupported_fields).collect(),
                expected_call_info,
            ]
            .concat()
            .into(),
        ),
        ..trivial_external_entry_point_with_address(test_contract_address)
    };
    let result = entry_point_call.execute_directly_given_tx_info(
        state,
        tx_info,
        None,
        false,
        execution_mode,
    );

    assert!(!result.unwrap().execution.failed);
}

#[test]
fn test_gas_types_constants() {
    assert_eq!(str_to_32_bytes_in_hex("L1_GAS"), Resource::L1Gas.to_hex());
    assert_eq!(str_to_32_bytes_in_hex("L2_GAS"), Resource::L2Gas.to_hex());
    assert_eq!(str_to_32_bytes_in_hex("L1_DATA"), Resource::L1DataGas.to_hex());
}

fn str_to_32_bytes_in_hex(s: &str) -> String {
    if s.len() > 32 {
        panic!("Unsupported input of length > 32.")
    }
    let prefix = "0x";
    let padding_zeros = "0".repeat(64 - s.len() * 2); // Each string char is 2 chars in hex.
    let word_in_hex: String =
        s.as_bytes().iter().fold(String::new(), |s, byte| s + (&format!("{byte:02x}")));
    [prefix, &padding_zeros, &word_in_hex].into_iter().collect()
}
