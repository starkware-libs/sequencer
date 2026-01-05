use std::sync::Arc;

use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use rstest::rstest;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::block::GasPrice;
use starknet_api::contract_class::compiled_class_hash::HashVersion;
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
    ProofFacts,
    Resource,
    ResourceBounds,
    Tip,
    TransactionSignature,
    ValidResourceBounds,
};
use starknet_api::transaction::{TransactionVersion, QUERY_VERSION_BASE};
use starknet_api::versioned_constants_logic::VersionedConstantsTrait;
use starknet_api::{felt, nonce, tx_hash};
use starknet_types_core::felt::Felt;

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
use crate::transaction::test_utils::proof_facts_as_cairo_array;

/// Test variant flags for `get_execution_info` syscall tests.
#[derive(Clone, Copy, Default)]
struct Variant {
    /// If true, the transaction is a query (dry-run) rather than an actual execution.
    only_query: bool,
    /// If true, the sender account is a "v1-bound" account that gets its version forced to V1
    v1_bound_account: bool,
    /// If true, the tip exceeds `v1_bound_accounts_max_tip`, which overrides the v1-bound behavior
    /// and allows V3 execution. Only valid when `v1_bound_account` is true.
    high_tip: bool,
    /// If true, the sender account excludes L1 data gas from resource bounds.
    exclude_l1_data_gas: bool,
}

impl Variant {
    const fn new(
        only_query: bool,
        v1_bound_account: bool,
        high_tip: bool,
        exclude_l1_data_gas: bool,
    ) -> Self {
        assert!(!high_tip || v1_bound_account, "high_tip requires v1_bound_account");
        assert!(
            !(v1_bound_account && exclude_l1_data_gas),
            "v1_bound_account and exclude_l1_data_gas are mutually exclusive"
        );
        Self { only_query, v1_bound_account, high_tip, exclude_l1_data_gas }
    }
}

fn test_get_execution_info(
    test_contract: FeatureContract,
    execution_mode: ExecutionMode,
    version: TransactionVersion,
    variant: &Variant,
) {
    let Variant { only_query, v1_bound_account, high_tip, exclude_l1_data_gas, .. } = *variant;

    let mut test_contract_data: FeatureContractData = test_contract.into();

    // Override class hash for special account types that affect execution info behavior.
    if v1_bound_account {
        assert!(
            !exclude_l1_data_gas,
            "Unable to set both exclude_l1_data_gas and v1_bound_account."
        );
        test_contract_data.class_hash = *VersionedConstants::latest_constants()
            .os_constants
            .v1_bound_accounts_cairo1
            .first()
            .expect("No v1 bound accounts found in versioned constants.");
    } else if exclude_l1_data_gas {
        test_contract_data.class_hash =
            *VersionedConstants::latest_constants().os_constants.data_gas_accounts.first().unwrap();
    }

    let erc20_version = test_contract.cairo_version();
    let state = &mut test_state_inner(
        &ChainInfo::create_for_testing(),
        BALANCE,
        &[(test_contract_data, 1)],
        &HashVersion::V2,
        erc20_version,
    );

    // Build expected block info.
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

    // Build transaction fields.
    let test_contract_address = test_contract.get_instance_address(0);
    let tx_hash = tx_hash!(1991);
    let max_fee = if version == TransactionVersion::ONE { Fee(42) } else { Fee(0) };
    let nonce = nonce!(3_u16);
    let sender_address = test_contract_address;
    let tip = Tip(VersionedConstants::latest_constants().os_constants.v1_bound_accounts_max_tip.0
        + if high_tip { 1 } else { 0 });
    let expected_tip = if version == TransactionVersion::THREE { tip } else { Tip(0) };

    let resource_bounds =
        ResourceBounds { max_amount: GasAmount(13), max_price_per_unit: GasPrice(61) };
    let all_resource_bounds = ValidResourceBounds::AllResources(AllResourceBounds {
        l1_gas: resource_bounds,
        l2_gas: resource_bounds,
        l1_data_gas: resource_bounds,
    });

    let expected_signature = match test_contract {
        FeatureContract::LegacyTestContract => vec![],
        #[cfg(feature = "cairo_native")]
        FeatureContract::SierraExecutionInfoV1Contract(RunnableCairo1::Native) => vec![],
        FeatureContract::TestContract(_) => vec![tx_hash.0],
        _ => panic!("Unsupported contract for this test."),
    };
    let signature = TransactionSignature(Arc::new(expected_signature));

    let mut expected_version = if v1_bound_account && !high_tip { 1.into() } else { version.0 };
    if only_query {
        expected_version += *QUERY_VERSION_BASE;
    }

    // Only V3 transactions support non-trivial proof facts.
    let proof_facts = if version == TransactionVersion::THREE {
        ProofFacts::snos_proof_facts_for_testing()
    } else {
        ProofFacts::default()
    };

    // Build transaction info object.
    let common_fields = CommonAccountFields {
        transaction_hash: tx_hash,
        version,
        signature,
        nonce,
        sender_address,
        only_query,
    };

    let tx_info = if version == TransactionVersion::ONE {
        TransactionInfo::Deprecated(DeprecatedTransactionInfo { common_fields, max_fee })
    } else {
        TransactionInfo::Current(CurrentTransactionInfo {
            common_fields,
            resource_bounds: all_resource_bounds,
            tip,
            nonce_data_availability_mode: DataAvailabilityMode::L1,
            fee_data_availability_mode: DataAvailabilityMode::L1,
            paymaster_data: PaymasterData::default(),
            account_deployment_data: AccountDeploymentData::default(),
            proof_facts: proof_facts.clone(),
        })
    };

    // Build expected calldata to pass to the contract's test_get_execution_info entry point.
    // The contract will compare the syscall results with the expected values.
    let entry_point_selector = selector_from_name("test_get_execution_info");

    let expected_call_info = vec![
        felt!(0_u16), // Caller address.
        *test_contract_address.0.key(),
        entry_point_selector.0,
    ];

    // TxInfo fields (shared between V1 and V3).
    let expected_tx_info = vec![
        expected_version,
        *sender_address.0.key(),
        felt!(max_fee.0),
        Felt::ZERO, // Signature
        tx_hash.0,
        felt!(&*CHAIN_ID_FOR_TESTS.as_hex()),
        nonce.0,
    ];

    let mut calldata = vec![expected_block_info.to_vec(), expected_call_info, expected_tx_info];

    // TestContract uses get_execution_info_v3 which includes additional V3 fields.
    // The LegacyTestContract and SierraExecutionInfoV1Contract use get_execution_info_v1 and don't
    // expect these fields.
    if matches!(test_contract, FeatureContract::TestContract(_)) {
        let expected_resource_bounds: Vec<Felt> = if version == TransactionVersion::ONE {
            vec![felt!(0_u16)] // Empty resource bounds for V1.
        } else {
            let num_resources = if exclude_l1_data_gas { 2_u8 } else { 3_u8 };
            std::iter::once(felt!(num_resources))
                .chain(
                    valid_resource_bounds_as_felts(&all_resource_bounds, exclude_l1_data_gas)
                        .unwrap()
                        .into_iter()
                        .flat_map(|bounds| bounds.flatten()),
                )
                .collect()
        };

        let expected_unsupported_fields = vec![
            expected_tip.into(),
            Felt::ZERO, // Paymaster data.
            Felt::ZERO, // Nonce DA mode.
            Felt::ZERO, // Fee DA mode.
            Felt::ZERO, // Account deployment data.
        ];

        calldata.push(
            expected_resource_bounds.into_iter().chain(expected_unsupported_fields).collect(),
        );
        calldata.push(proof_facts_as_cairo_array(proof_facts));
    }

    // Execute and verify.
    let entry_point_call = CallEntryPoint {
        entry_point_selector,
        code_address: None,
        calldata: Calldata(calldata.concat().into()),
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

/// Tests `get_execution_info` for `TestContract` (Cairo 1), which uses
/// `get_execution_info_v3_syscall()` and expects V3 `TxInfo`.
///
/// Covers execution modes (`Validate` vs `Execute`), tx versions (V1 vs V3), and query mode.
/// In `Validate`, block info fields are rounded/zeroed; in `Execute`, they are populated normally.
#[rstest]
#[cfg_attr(feature = "cairo_native", case(RunnableCairo1::Native, TransactionVersion::ONE, false))]
#[cfg_attr(feature = "cairo_native", case(RunnableCairo1::Native, TransactionVersion::THREE, false))]
#[cfg_attr(feature = "cairo_native", case(RunnableCairo1::Native, TransactionVersion::THREE, true))]
#[case(RunnableCairo1::Casm, TransactionVersion::ONE, false)]
#[case(RunnableCairo1::Casm, TransactionVersion::THREE, false)]
#[case(RunnableCairo1::Casm, TransactionVersion::THREE, true)]
fn test_supported_get_execution_info(
    #[case] runnable: RunnableCairo1,
    #[case] version: TransactionVersion,
    #[case] only_query: bool,
    #[values(ExecutionMode::Validate, ExecutionMode::Execute)] execution_mode: ExecutionMode,
) {
    let contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable));
    test_get_execution_info(
        contract,
        execution_mode,
        version,
        &Variant::new(only_query, false, false, false),
    )
}

/// Tests `get_execution_info` for legacy contracts that use V1 `TxInfo`:
/// - `SierraExecutionInfoV1Contract` (Cairo 1): uses `get_execution_info_syscall()`.
/// - `LegacyTestContract` (Cairo 0, compiler v2.1.0): uses `get_execution_info()`.
///
/// These contracts only support the base variant (all flags false).
/// In `Validate`, block info fields are rounded/zeroed; in `Execute`, they are populated normally.
#[rstest]
// SierraExecutionInfoV1Contract (native only).
#[cfg_attr(
    feature = "cairo_native",
    case(
        FeatureContract::SierraExecutionInfoV1Contract(RunnableCairo1::Native),
        ExecutionMode::Validate,
        TransactionVersion::ONE
    )
)]
#[cfg_attr(
    feature = "cairo_native",
    case(
        FeatureContract::SierraExecutionInfoV1Contract(RunnableCairo1::Native),
        ExecutionMode::Execute,
        TransactionVersion::ONE
    )
)]
// LegacyTestContract.
#[cfg_attr(
    feature = "cairo_native",
    case(FeatureContract::LegacyTestContract, ExecutionMode::Execute, TransactionVersion::ONE)
)]
#[cfg_attr(
    feature = "cairo_native",
    case(FeatureContract::LegacyTestContract, ExecutionMode::Execute, TransactionVersion::THREE)
)]
#[case(FeatureContract::LegacyTestContract, ExecutionMode::Execute, TransactionVersion::ONE)]
#[case(FeatureContract::LegacyTestContract, ExecutionMode::Execute, TransactionVersion::THREE)]
fn test_legacy_get_execution_info(
    #[case] contract: FeatureContract,
    #[case] execution_mode: ExecutionMode,
    #[case] version: TransactionVersion,
) {
    // Sanity check: verify legacy contract has expected compiler version.
    if matches!(contract, FeatureContract::LegacyTestContract) {
        let raw_contract: serde_json::Value =
            serde_json::from_str(&contract.get_raw_class()).expect("Error parsing JSON");
        let compiler_version = raw_contract["compiler_version"]
            .as_str()
            .expect("'compiler_version' not found or not a valid string in JSON.");
        assert_eq!(compiler_version, "2.1.0");
    }

    test_get_execution_info(contract, execution_mode, version, &Variant::default());
}

/// Tests `get_execution_info` for v1-bound accounts.
///
/// V1-bound accounts have their version forced to V1 unless `high_tip` is set.
/// Only tests `Execute` mode with V3 transactions.
#[rstest]
#[cfg_attr(feature = "cairo_native", case(RunnableCairo1::Native, true, false))]
#[cfg_attr(feature = "cairo_native", case(RunnableCairo1::Native, false, true))]
#[case(RunnableCairo1::Casm, true, false)]
#[case(RunnableCairo1::Casm, false, true)]
fn test_v1_bound_account_get_execution_info(
    #[case] runnable: RunnableCairo1,
    #[case] only_query: bool,
    #[case] high_tip: bool,
) {
    let contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable));
    test_get_execution_info(
        contract,
        ExecutionMode::Execute,
        TransactionVersion::THREE,
        &Variant::new(only_query, true, high_tip, false),
    );
}

/// Tests `get_execution_info` for accounts that exclude L1 data gas from resource bounds.
///
/// These accounts only report 2 resource types instead of 3.
/// Only tests `Execute` mode with V3 transactions.
#[rstest]
#[cfg_attr(feature = "cairo_native", case(RunnableCairo1::Native))]
#[case(RunnableCairo1::Casm)]
fn test_exclude_l1_data_gas_get_execution_info(#[case] runnable: RunnableCairo1) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable));
    test_get_execution_info(
        test_contract,
        ExecutionMode::Execute,
        TransactionVersion::THREE,
        &Variant::new(false, false, false, true),
    );
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
