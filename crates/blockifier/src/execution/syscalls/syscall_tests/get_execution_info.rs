use std::sync::Arc;

use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use rstest::rstest;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::block::{BlockNumber, BlockTimestamp, GasPrice};
use starknet_api::contract_class::compiled_class_hash::HashVersion;
use starknet_api::core::ContractAddress;
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
    AccountDeploymentData,
    AllResourceBounds,
    Calldata,
    Fee,
    PaymasterData,
    ProofFacts,
    Resource,
    ResourceBounds,
    Tip,
    TransactionSignature,
    ValidResourceBounds,
};
use starknet_api::transaction::TransactionVersion;
use starknet_api::versioned_constants_logic::VersionedConstantsTrait;
use starknet_api::{contract_address, felt, nonce, tx_hash};
use starknet_types_core::felt::Felt;
use test_case::test_case;

use crate::blockifier_versioned_constants::VersionedConstants;
use crate::context::ChainInfo;
use crate::execution::common_hints::ExecutionMode;
use crate::execution::entry_point::CallEntryPoint;
use crate::test_utils::contracts::FeatureContractData;
use crate::test_utils::initial_test_state::test_state_inner;
use crate::test_utils::{trivial_external_entry_point_with_address, BALANCE};
use crate::transaction::objects::{
    CommonAccountFields,
    CurrentTransactionInfo,
    DeprecatedTransactionInfo,
    TransactionInfo,
};
use crate::transaction::test_utils::ExpectedExecutionInfo;

// =====================================================================================
// Helper functions for test setup
// =====================================================================================

/// Returns the standard resource bounds used across tests.
fn test_resource_bounds() -> ValidResourceBounds {
    let resource_bounds =
        ResourceBounds { max_amount: GasAmount(13), max_price_per_unit: GasPrice(61) };
    ValidResourceBounds::AllResources(AllResourceBounds {
        l1_gas: resource_bounds,
        l2_gas: resource_bounds,
        l1_data_gas: resource_bounds,
    })
}

/// Returns the default tip value used across tests.
fn default_tip() -> Tip {
    Tip(VersionedConstants::latest_constants().os_constants.v1_bound_accounts_max_tip.0)
}

/// Returns block info based on execution mode.
fn block_info_for_mode(
    execution_mode: ExecutionMode,
) -> (BlockNumber, BlockTimestamp, ContractAddress) {
    match execution_mode {
        ExecutionMode::Validate => (
            BlockNumber(CURRENT_BLOCK_NUMBER_FOR_VALIDATE),
            BlockTimestamp(CURRENT_BLOCK_TIMESTAMP_FOR_VALIDATE),
            ContractAddress::default(),
        ),
        ExecutionMode::Execute => (
            BlockNumber(CURRENT_BLOCK_NUMBER),
            BlockTimestamp(CURRENT_BLOCK_TIMESTAMP),
            contract_address!(TEST_SEQUENCER_ADDRESS),
        ),
    }
}

/// Returns expected block info as Felts based on execution mode.
fn expected_block_info_as_felts(execution_mode: ExecutionMode) -> Vec<Felt> {
    let (block_number, block_timestamp, sequencer_address) = block_info_for_mode(execution_mode);
    vec![felt!(block_number.0), felt!(block_timestamp.0), *sequencer_address.0.key()]
}

/// Creates a V3 CurrentTransactionInfo with common defaults.
#[allow(clippy::too_many_arguments)]
fn create_current_tx_info(
    tx_hash: starknet_api::transaction::TransactionHash,
    nonce: starknet_api::core::Nonce,
    sender_address: ContractAddress,
    resource_bounds: ValidResourceBounds,
    tip: Tip,
    proof_facts: ProofFacts,
    signature: TransactionSignature,
    only_query: bool,
) -> TransactionInfo {
    TransactionInfo::Current(CurrentTransactionInfo {
        common_fields: CommonAccountFields {
            transaction_hash: tx_hash,
            version: TransactionVersion::THREE,
            signature,
            nonce,
            sender_address,
            only_query,
        },
        resource_bounds,
        tip,
        nonce_data_availability_mode: DataAvailabilityMode::L1,
        fee_data_availability_mode: DataAvailabilityMode::L1,
        paymaster_data: PaymasterData::default(),
        account_deployment_data: AccountDeploymentData::default(),
        proof_facts,
    })
}

/// Creates a V1 DeprecatedTransactionInfo with common defaults.
fn create_deprecated_tx_info(
    tx_hash: starknet_api::transaction::TransactionHash,
    nonce: starknet_api::core::Nonce,
    sender_address: ContractAddress,
    max_fee: Fee,
    signature: TransactionSignature,
    only_query: bool,
) -> TransactionInfo {
    TransactionInfo::Deprecated(DeprecatedTransactionInfo {
        common_fields: CommonAccountFields {
            transaction_hash: tx_hash,
            version: TransactionVersion::ONE,
            signature,
            nonce,
            sender_address,
            only_query,
        },
        max_fee,
    })
}

// =====================================================================================
// Standard TestContract tests (V1 + V3) using ExpectedExecutionInfo
// Also includes special accounts: v1_bound_account and exclude_l1_data_gas.
// =====================================================================================

/// Special account types that modify execution info behavior.
#[derive(Clone, Copy, Debug, PartialEq)]
enum SpecialAccount {
    /// Standard account with no special behavior.
    None,
    /// V1 bound account: shows V1 version if tip <= max_tip, else V3.
    V1Bound,
    /// V1 bound account with high tip: shows V3 version.
    V1BoundHighTip,
    /// Data gas account: excludes L1 data gas from resource bounds (2 types instead of 3).
    ExcludeL1DataGas,
}

#[test_case(ExecutionMode::Validate, TransactionVersion::ONE, false, SpecialAccount::None; "Validate V1")]
#[test_case(ExecutionMode::Execute, TransactionVersion::ONE, false, SpecialAccount::None; "Execute V1")]
#[test_case(ExecutionMode::Validate, TransactionVersion::THREE, false, SpecialAccount::None; "Validate V3")]
#[test_case(ExecutionMode::Execute, TransactionVersion::THREE, false, SpecialAccount::None; "Execute V3")]
#[test_case(ExecutionMode::Execute, TransactionVersion::THREE, true, SpecialAccount::None; "Execute V3 Query")]
#[test_case(ExecutionMode::Execute, TransactionVersion::THREE, false, SpecialAccount::V1Bound; "v1_bound execute")]
#[test_case(ExecutionMode::Execute, TransactionVersion::THREE, true, SpecialAccount::V1Bound; "v1_bound query")]
#[test_case(ExecutionMode::Execute, TransactionVersion::THREE, false, SpecialAccount::V1BoundHighTip; "v1_bound high_tip")]
#[test_case(ExecutionMode::Execute, TransactionVersion::THREE, false, SpecialAccount::ExcludeL1DataGas; "exclude_l1_data execute")]
#[test_case(ExecutionMode::Execute, TransactionVersion::THREE, true, SpecialAccount::ExcludeL1DataGas; "exclude_l1_data query")]
fn test_get_execution_info(
    execution_mode: ExecutionMode,
    version: TransactionVersion,
    only_query: bool,
    special_account: SpecialAccount,
) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let mut test_contract_data: FeatureContractData = test_contract.into();

    // Set class hash for special accounts.
    match special_account {
        SpecialAccount::None => {}
        SpecialAccount::V1Bound | SpecialAccount::V1BoundHighTip => {
            test_contract_data.class_hash = *VersionedConstants::latest_constants()
                .os_constants
                .v1_bound_accounts_cairo1
                .first()
                .expect("No v1 bound accounts found in versioned constants.");
        }
        SpecialAccount::ExcludeL1DataGas => {
            test_contract_data.class_hash = *VersionedConstants::latest_constants()
                .os_constants
                .data_gas_accounts
                .first()
                .unwrap();
        }
    }

    let state = &mut test_state_inner(
        &ChainInfo::create_for_testing(),
        BALANCE,
        &[(test_contract_data, 1)],
        &HashVersion::V2,
        test_contract.cairo_version(),
    );

    let test_contract_address = test_contract.get_instance_address(0);
    let entry_point_selector = selector_from_name("test_get_execution_info");
    let (block_number, block_timestamp, sequencer_address) = block_info_for_mode(execution_mode);
    let all_resource_bounds = test_resource_bounds();
    let nonce = nonce!(3_u16);
    let max_fee = Fee(42);
    let tx_hash = tx_hash!(1991);

    // Tip: high_tip adds 1 to exceed the max_tip threshold.
    let tip = if special_account == SpecialAccount::V1BoundHighTip {
        Tip(default_tip().0 + 1)
    } else {
        default_tip()
    };

    // Proof facts: only standard V3 transactions use non-trivial proof facts.
    let proof_facts = if version == TransactionVersion::THREE {
        ProofFacts::snos_proof_facts_for_testing()
    } else {
        ProofFacts::default()
    };

    // Build expected execution info using the helper struct.
    let mut expected = ExpectedExecutionInfo::new(
        only_query,
        test_contract_address,
        ContractAddress::default(), // caller_address
        test_contract_address,
        CHAIN_ID_FOR_TESTS.clone(),
        entry_point_selector,
        block_number,
        block_timestamp,
        sequencer_address,
        all_resource_bounds,
        nonce,
        proof_facts.clone(),
    );

    // Apply version and special account modifications.
    if version == TransactionVersion::ONE {
        expected = expected.with_v1(max_fee);
    } else if special_account == SpecialAccount::V1Bound {
        // V1 bound with tip <= max_tip: shows V1 version but uses V3 resource bounds.
        expected = expected.with_tip(tip).with_v1_display_v3_bounds(only_query);
    } else if special_account == SpecialAccount::ExcludeL1DataGas {
        expected = expected.with_tip(tip).with_exclude_l1_data_gas();
    } else {
        // Standard V3 or V1BoundHighTip (which shows V3 version).
        expected = expected.with_tip(tip);
    }

    let calldata = Calldata(expected.to_syscall_result().into());

    // Build the transaction info for execution.
    let signature = TransactionSignature(Arc::new(vec![tx_hash.0]));
    let tx_info = if version == TransactionVersion::THREE {
        create_current_tx_info(
            tx_hash,
            nonce,
            test_contract_address,
            all_resource_bounds,
            tip,
            proof_facts,
            signature,
            only_query,
        )
    } else {
        create_deprecated_tx_info(
            tx_hash,
            nonce,
            test_contract_address,
            max_fee,
            signature,
            only_query,
        )
    };

    let entry_point_call = CallEntryPoint {
        entry_point_selector,
        code_address: None,
        calldata,
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

/// =====================================================================================
// Legacy contracts test (LegacyTestContract, SierraExecutionInfoV1Contract)
// These contracts have a different calldata format: no unsupported_fields, no proof_facts.
// =====================================================================================

#[rstest]
#[cfg_attr(
    feature = "cairo_native",
    case::native_sierra_validate(FeatureContract::SierraExecutionInfoV1Contract(
        RunnableCairo1::Native
    ))
)]
#[case::legacy(FeatureContract::LegacyTestContract)]
fn test_get_execution_info_legacy(
    #[case] test_contract: FeatureContract,
    #[values(ExecutionMode::Validate, ExecutionMode::Execute)] execution_mode: ExecutionMode,
    #[values(TransactionVersion::ONE, TransactionVersion::THREE)] version: TransactionVersion,
) {
    let test_contract_data: FeatureContractData = test_contract.into();
    let state = &mut test_state_inner(
        &ChainInfo::create_for_testing(),
        BALANCE,
        &[(test_contract_data, 1)],
        &HashVersion::V2,
        test_contract.cairo_version(),
    );

    let test_contract_address = test_contract.get_instance_address(0);
    let entry_point_selector = selector_from_name("test_get_execution_info");
    let tx_hash = tx_hash!(1991);
    let max_fee = Fee(42);
    let nonce = nonce!(3_u16);

    // Verify LegacyTestContract compiler version.
    if matches!(test_contract, FeatureContract::LegacyTestContract) {
        let raw_contract: serde_json::Value =
            serde_json::from_str(&test_contract.get_raw_class()).expect("Error parsing JSON");
        assert_eq!(raw_contract["compiler_version"].as_str(), Some("2.1.0"));
    }

    let expected_block_info = expected_block_info_as_felts(execution_mode);

    // Legacy contracts: empty unsupported_fields and empty signature.
    let expected_tx_info = vec![
        version.0,
        *test_contract_address.0.key(),
        if version == TransactionVersion::ONE { felt!(max_fee.0) } else { Felt::ZERO },
        Felt::ZERO, // Empty signature length.
        tx_hash.0,
        felt!(&*CHAIN_ID_FOR_TESTS.as_hex()),
        nonce.0,
    ];

    // Legacy contracts have empty resource bounds and unsupported fields.
    let calldata = Calldata(
        [
            expected_block_info,
            expected_tx_info,
            vec![],
            vec![Felt::ZERO, *test_contract_address.0.key(), entry_point_selector.0],
        ]
        .concat()
        .into(),
    );

    // Legacy contracts use empty signature.
    let signature = TransactionSignature(Arc::new(vec![]));
    let tx_info = if version == TransactionVersion::THREE {
        create_current_tx_info(
            tx_hash,
            nonce,
            test_contract_address,
            test_resource_bounds(),
            Tip(0),
            ProofFacts::default(),
            signature,
            false,
        )
    } else {
        create_deprecated_tx_info(tx_hash, nonce, test_contract_address, max_fee, signature, false)
    };

    let entry_point_call = CallEntryPoint {
        entry_point_selector,
        code_address: None,
        calldata,
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
