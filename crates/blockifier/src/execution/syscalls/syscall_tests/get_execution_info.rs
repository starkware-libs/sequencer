use cairo_vm::Felt252;
use num_traits::Pow;
use starknet_api::block::GasPrice;
use starknet_api::core::ChainId;
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::execution_resources::GasAmount;
use starknet_api::transaction::{
    AccountDeploymentData,
    Calldata,
    Fee,
    PaymasterData,
    Resource,
    ResourceBounds,
    Tip,
    TransactionHash,
    TransactionVersion,
    ValidResourceBounds,
    QUERY_VERSION_BASE_BIT,
};
use starknet_api::{felt, nonce};
use starknet_types_core::felt::Felt;
use test_case::test_case;

use crate::abi::abi_utils::selector_from_name;
use crate::context::ChainInfo;
use crate::execution::common_hints::ExecutionMode;
use crate::execution::entry_point::CallEntryPoint;
use crate::test_utils::contracts::FeatureContract;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::{
    trivial_external_entry_point_with_address,
    CairoVersion,
    BALANCE,
    CURRENT_BLOCK_NUMBER,
    CURRENT_BLOCK_NUMBER_FOR_VALIDATE,
    CURRENT_BLOCK_TIMESTAMP,
    CURRENT_BLOCK_TIMESTAMP_FOR_VALIDATE,
    TEST_SEQUENCER_ADDRESS,
};
use crate::transaction::objects::{
    CommonAccountFields,
    CurrentTransactionInfo,
    DeprecatedTransactionInfo,
    TransactionInfo,
};

#[test_case(
    FeatureContract::TestContract(CairoVersion::Cairo1),
    ExecutionMode::Validate,
    TransactionVersion::ONE,
    false;
    "Validate execution mode: block info fields should be zeroed. Transaction V1.")]
#[test_case(
    FeatureContract::TestContract(CairoVersion::Cairo1),
    ExecutionMode::Execute,
    TransactionVersion::ONE,
    false;
    "Execute execution mode: block info should be as usual. Transaction V1.")]
#[test_case(
    FeatureContract::TestContract(CairoVersion::Cairo1),
    ExecutionMode::Validate,
    TransactionVersion::THREE,
    false;
    "Validate execution mode: block info fields should be zeroed. Transaction V3.")]
#[test_case(
    FeatureContract::TestContract(CairoVersion::Cairo1),
    ExecutionMode::Execute,
    TransactionVersion::THREE,
    false;
    "Execute execution mode: block info should be as usual. Transaction V3.")]
#[test_case(
    FeatureContract::LegacyTestContract,
    ExecutionMode::Execute,
    TransactionVersion::ONE,
    false;
    "Legacy contract. Execute execution mode: block info should be as usual. Transaction V1.")]
#[test_case(
    FeatureContract::LegacyTestContract,
    ExecutionMode::Execute,
    TransactionVersion::THREE,
    false;
    "Legacy contract. Execute execution mode: block info should be as usual. Transaction V3.")]
#[test_case(
    FeatureContract::TestContract(CairoVersion::Cairo1),
    ExecutionMode::Execute,
    TransactionVersion::THREE,
    true;
    "Execute execution mode: block info should be as usual. Transaction V3. Query.")]
fn test_get_execution_info(
    test_contract: FeatureContract,
    execution_mode: ExecutionMode,
    mut version: TransactionVersion,
    only_query: bool,
) {
    let state = &mut test_state(&ChainInfo::create_for_testing(), BALANCE, &[(test_contract, 1)]);
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
        _ => {
            vec![
                Felt::ZERO, // Tip.
                Felt::ZERO, // Paymaster data.
                Felt::ZERO, // Nonce DA.
                Felt::ZERO, // Fee DA.
                Felt::ZERO, // Account data.
            ]
        }
    };

    if only_query {
        let simulate_version_base = Pow::pow(Felt252::from(2_u8), QUERY_VERSION_BASE_BIT);
        let query_version = simulate_version_base + version.0;
        version = TransactionVersion(query_version);
    }

    let tx_hash = TransactionHash(felt!(1991_u16));
    let max_fee = Fee(42 * crate::test_utils::DEFAULT_ETH_L1_GAS_PRICE.get().0);
    let nonce = nonce!(3_u16);
    let sender_address = test_contract_address;

    let max_amount = GasAmount(13);
    let max_price_per_unit = GasPrice(61);

    let expected_resource_bounds: Vec<Felt> = match (test_contract, version) {
        (FeatureContract::LegacyTestContract, _) => vec![],
        (_, version) if version == TransactionVersion::ONE => vec![
            felt!(0_u16), // Length of resource bounds array.
        ],
        (_, _) => vec![
            Felt::from(2u32),                // Length of ResourceBounds array.
            felt!(Resource::L1Gas.to_hex()), // Resource.
            max_amount.into(),               // Max amount.
            max_price_per_unit.into(),       // Max price per unit.
            felt!(Resource::L2Gas.to_hex()), // Resource.
            Felt::ZERO,                      // Max amount.
            Felt::ZERO,                      // Max price per unit.
        ],
    };

    let expected_tx_info: Vec<Felt>;
    let tx_info: TransactionInfo;
    if version == TransactionVersion::ONE {
        expected_tx_info = vec![
            version.0,                                       /* Transaction
                                                              * version. */
            *sender_address.0.key(), // Account address.
            felt!(max_fee.0),        // Max fee.
            Felt::ZERO,              // Signature.
            tx_hash.0,               // Transaction hash.
            felt!(&*ChainId::create_for_testing().as_hex()), // Chain ID.
            nonce.0,                 // Nonce.
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
            version.0,                                       /* Transaction
                                                              * version. */
            *sender_address.0.key(), // Account address.
            Felt::ZERO,              // Max fee.
            Felt::ZERO,              // Signature.
            tx_hash.0,               // Transaction hash.
            felt!(&*ChainId::create_for_testing().as_hex()), // Chain ID.
            nonce.0,                 // Nonce.
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
            resource_bounds: ValidResourceBounds::L1Gas(ResourceBounds {
                max_amount,
                max_price_per_unit,
            }),
            tip: Tip::default(),
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

    let result = match execution_mode {
        ExecutionMode::Validate => {
            entry_point_call.execute_directly_given_tx_info_in_validate_mode(state, tx_info)
        }
        ExecutionMode::Execute => entry_point_call.execute_directly_given_tx_info(state, tx_info),
    };

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
        s.as_bytes().iter().fold(String::new(), |s, byte| s + (&format!("{:02x}", byte)));
    [prefix, &padding_zeros, &word_in_hex].into_iter().collect()
}
