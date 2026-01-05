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
use crate::state::cached_state::CachedState;
use crate::state::state_api::StateReader;
use crate::test_utils::contracts::FeatureContractData;
use crate::test_utils::dict_state_reader::DictStateReader;
use crate::test_utils::initial_test_state::test_state_inner;
use crate::test_utils::{trivial_external_entry_point_with_address, BALANCE};
use crate::transaction::objects::{
    CommonAccountFields,
    CurrentTransactionInfo,
    DeprecatedTransactionInfo,
    TransactionInfo,
};
use crate::transaction::test_utils::proof_facts_as_cairo_array;

/// Flags controlling `get_execution_info` syscall test behavior.
#[derive(Clone, Copy, Default)]
struct ExecutionInfoTestFlags {
    /// Whether the transaction is a query (dry-run) rather than an actual execution.
    only_query: bool,
    /// Whether the sender is a "v1-bound account" - an account with hardcoded version
    /// assertions that cannot handle V3 transactions. The gateway only accepts V3 transactions,
    /// but for these accounts, execution info reports V1.
    is_v1_bound_account: bool,
    /// Only meaningful when `is_v1_bound_account` is true.
    tip_exceeds_v1_bound_threshold: bool,
    /// Whether the sender is in the "data gas accounts" list, causing L1 data gas to be
    /// excluded from reported resource bounds.
    exclude_l1_data_gas: bool,
}

/// Contains all data needed to execute and verify a `get_execution_info` test.
struct TestSetup<S: StateReader> {
    state: CachedState<S>,
    tx_info: TransactionInfo,
    execution_mode: ExecutionMode,
    entry_point_selector: starknet_api::core::EntryPointSelector,
    test_contract_address: starknet_api::core::ContractAddress,
    /// Calldata segments (block info, call info, tx info). Extended for contracts using
    /// `get_execution_info_v3_syscall`, which returns V3 `TxInfo` with additional fields.
    calldata: Vec<Vec<Felt>>,
    // Fields used when extending calldata for post-V1 execution info syscall.
    exclude_l1_data_gas: bool,
    expected_tip: Tip,
}

impl<S: StateReader> TestSetup<S> {
    /// Builds the final calldata and returns the configured entry point.
    fn build_entry_point(&self) -> CallEntryPoint {
        CallEntryPoint {
            entry_point_selector: self.entry_point_selector,
            code_address: None,
            calldata: Calldata(self.calldata.concat().into()),
            ..trivial_external_entry_point_with_address(self.test_contract_address)
        }
    }

    /// Executes the entry point and asserts the execution succeeded.
    fn execute_and_assert(&mut self) {
        let entry_point = self.build_entry_point();
        let result = entry_point.execute_directly_given_tx_info(
            &mut self.state,
            self.tx_info.clone(),
            None,
            false,
            self.execution_mode,
        );
        assert!(!result.unwrap().execution.failed);
    }

    /// Extends calldata with post-V1 TxInfo fields: resource bounds, unsupported fields,
    /// and proof facts.
    fn extend_calldata_for_post_v1_execution_info_syscall(&mut self) {
        let (expected_resource_bounds, proof_facts) = match &self.tx_info {
            TransactionInfo::Deprecated(_) => (vec![felt!(0_u16)], ProofFacts::default()),
            TransactionInfo::Current(info) => {
                let num_resources = if self.exclude_l1_data_gas { 2_u8 } else { 3_u8 };
                let bounds = std::iter::once(felt!(num_resources))
                    .chain(
                        valid_resource_bounds_as_felts(
                            &info.resource_bounds,
                            self.exclude_l1_data_gas,
                        )
                        .unwrap()
                        .into_iter()
                        .flat_map(|bounds| bounds.flatten()),
                    )
                    .collect();
                (bounds, info.proof_facts.clone())
            }
        };

        let expected_unsupported_fields = vec![
            self.expected_tip.into(),
            Felt::ZERO, // Paymaster data.
            Felt::ZERO, // Nonce DA mode.
            Felt::ZERO, // Fee DA mode.
            Felt::ZERO, // Account deployment data.
        ];

        self.calldata.push(
            expected_resource_bounds.into_iter().chain(expected_unsupported_fields).collect(),
        );
        self.calldata.push(proof_facts_as_cairo_array(proof_facts));
    }
}

/// Computes the transaction version reported to the contract via execution info.
///
/// V1-bound accounts report V1 version (they would fail on V3), unless the tip exceeds the
/// threshold - in which case V3 is reported to prevent fee manipulation (V1 TxInfo excludes tips).
/// Query transactions have the query version base added.
fn compute_expected_tx_version(
    tx_version: TransactionVersion,
    flags: &ExecutionInfoTestFlags,
) -> Felt {
    let is_v1_bound_without_high_tip =
        flags.is_v1_bound_account && !flags.tip_exceeds_v1_bound_threshold;
    let mut version = if is_v1_bound_without_high_tip { 1.into() } else { tx_version.0 };
    if flags.only_query {
        version += *QUERY_VERSION_BASE;
    }
    version
}

/// Creates a test setup with all shared configuration.
fn create_test_setup(
    test_contract: FeatureContract,
    execution_mode: ExecutionMode,
    tx_version: TransactionVersion,
    flags: &ExecutionInfoTestFlags,
    class_hash_override: Option<starknet_api::core::ClassHash>,
) -> TestSetup<DictStateReader> {
    let ExecutionInfoTestFlags {
        only_query,
        is_v1_bound_account: _,
        tip_exceeds_v1_bound_threshold,
        exclude_l1_data_gas,
        ..
    } = *flags;

    let mut test_contract_data: FeatureContractData = test_contract.into();

    // Apply class hash override if provided (used for v1-bound and data-gas account tests).
    if let Some(class_hash) = class_hash_override {
        test_contract_data.class_hash = class_hash;
    }

    let erc20_version = test_contract.cairo_version();
    let state = test_state_inner(
        &ChainInfo::create_for_testing(),
        BALANCE,
        &[(test_contract_data, 1)],
        &HashVersion::V2,
        erc20_version,
    );

    // Build expected block info.
    let expected_block_info = match execution_mode {
        ExecutionMode::Validate => [
            felt!(CURRENT_BLOCK_NUMBER_FOR_VALIDATE),
            felt!(CURRENT_BLOCK_TIMESTAMP_FOR_VALIDATE),
            Felt::ZERO,
        ],
        ExecutionMode::Execute => [
            felt!(CURRENT_BLOCK_NUMBER),
            felt!(CURRENT_BLOCK_TIMESTAMP),
            Felt::from_hex(TEST_SEQUENCER_ADDRESS).unwrap(),
        ],
    };

    // Build transaction fields.
    let test_contract_address = test_contract.get_instance_address(0);
    let tx_hash = tx_hash!(1991);
    let max_fee = if tx_version == TransactionVersion::ONE { Fee(42) } else { Fee(0) };
    let nonce = nonce!(3_u16);
    let sender_address = test_contract_address;
    let tip = Tip(VersionedConstants::latest_constants().os_constants.v1_bound_accounts_max_tip.0
        + if tip_exceeds_v1_bound_threshold { 1 } else { 0 });
    let expected_tip = if tx_version == TransactionVersion::THREE { tip } else { Tip(0) };

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
    let expected_version = compute_expected_tx_version(tx_version, flags);

    // Only V3 transactions support non-trivial proof facts.
    let proof_facts = if tx_version == TransactionVersion::THREE {
        ProofFacts::snos_proof_facts_for_testing()
    } else {
        ProofFacts::default()
    };

    // Build transaction info object.
    let common_fields = CommonAccountFields {
        transaction_hash: tx_hash,
        version: tx_version,
        signature,
        nonce,
        sender_address,
        only_query,
    };

    let tx_info = if tx_version == TransactionVersion::ONE {
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
            proof_facts,
        })
    };

    // Build expected calldata (what the contract expects to receive via execution info).
    let entry_point_selector = selector_from_name("test_get_execution_info");

    let expected_call_info = vec![
        felt!(0_u16), // Caller address.
        *test_contract_address.0.key(),
        entry_point_selector.0,
    ];

    let expected_tx_info = vec![
        expected_version,
        *sender_address.0.key(),
        felt!(max_fee.0),
        Felt::ZERO, // Signature length (empty in this test).
        tx_hash.0,
        felt!(&*CHAIN_ID_FOR_TESTS.as_hex()),
        nonce.0,
    ];

    let calldata = vec![expected_block_info.to_vec(), expected_call_info, expected_tx_info];

    TestSetup {
        state,
        tx_info,
        execution_mode,
        entry_point_selector,
        test_contract_address,
        calldata,
        exclude_l1_data_gas,
        expected_tip,
    }
}

/// Tests `get_execution_info` for `TestContract` (Cairo 1), which uses
/// `get_execution_info_v3_syscall` and expects V3 `TxInfo`.
///
/// Covers execution modes (Validate vs Execute), transaction versions (V1 vs V3), and query mode.
/// In Validate mode, block info fields are rounded/zeroed; in Execute mode, they are fully
/// populated.
#[rstest]
#[cfg_attr(feature = "cairo_native", case(RunnableCairo1::Native, TransactionVersion::ONE, false))]
#[cfg_attr(
    feature = "cairo_native",
    case(RunnableCairo1::Native, TransactionVersion::THREE, false)
)]
#[cfg_attr(feature = "cairo_native", case(RunnableCairo1::Native, TransactionVersion::THREE, true))]
#[case(RunnableCairo1::Casm, TransactionVersion::ONE, false)]
#[case(RunnableCairo1::Casm, TransactionVersion::THREE, false)]
#[case(RunnableCairo1::Casm, TransactionVersion::THREE, true)]
fn test_get_execution_info(
    #[case] runnable: RunnableCairo1,
    #[case] tx_version: TransactionVersion,
    #[case] only_query: bool,
    #[values(ExecutionMode::Validate, ExecutionMode::Execute)] execution_mode: ExecutionMode,
) {
    let contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable));
    let flags = ExecutionInfoTestFlags { only_query, ..Default::default() };
    let mut setup = create_test_setup(contract, execution_mode, tx_version, &flags, None);
    setup.extend_calldata_for_post_v1_execution_info_syscall();
    setup.execute_and_assert();
}

/// Tests `get_execution_info` for legacy contracts that use the V1 syscall (returning V1 TxInfo):
/// - `SierraExecutionInfoV1Contract` (Cairo 1): uses `get_execution_info_syscall` (V1 syscall).
/// - `LegacyTestContract` (Cairo 0, compiler v2.1.0): uses the `get_execution_info` library
///   function.
///
/// These contracts only support default flags (all false) since they do not receive V3 TxInfo
/// fields.
#[rstest]
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
fn test_execution_info_v1_syscall(
    #[case] contract: FeatureContract,
    #[case] execution_mode: ExecutionMode,
    #[case] tx_version: TransactionVersion,
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

    let mut setup = create_test_setup(
        contract,
        execution_mode,
        tx_version,
        &ExecutionInfoTestFlags::default(),
        None,
    );
    setup.execute_and_assert();
}

/// Tests `get_execution_info` for V1-bound accounts - accounts with hardcoded version assertions
/// that cannot handle V3 transactions. Although the gateway only accepts V3 transactions,
/// execution info reports V1 for these accounts.
///
/// Exception: if the tip exceeds the threshold, V3 is reported to prevent fee manipulation
/// (V1 TxInfo excludes tips from fee calculations).
///
/// Only tests Execute mode; the main test covers both modes. This test focuses on V1-bound
/// behavior.
#[rstest]
#[cfg_attr(feature = "cairo_native", case(RunnableCairo1::Native))]
#[cfg_attr(feature = "cairo_native", case(RunnableCairo1::Native))]
#[case(RunnableCairo1::Casm)]
#[case(RunnableCairo1::Casm)]
fn test_v1_bound_account_get_execution_info(
    #[case] runnable: RunnableCairo1,
    #[values(true, false)] only_query: bool,
    #[values(true, false)] tip_exceeds_v1_bound_threshold: bool,
) {
    let contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable));
    let flags = ExecutionInfoTestFlags {
        only_query,
        is_v1_bound_account: true,
        tip_exceeds_v1_bound_threshold,
        ..Default::default()
    };
    let class_hash = *VersionedConstants::latest_constants()
        .os_constants
        .v1_bound_accounts_cairo1
        .first()
        .expect("No v1 bound accounts found in versioned constants.");
    let mut setup = create_test_setup(
        contract,
        ExecutionMode::Execute,
        TransactionVersion::THREE,
        &flags,
        Some(class_hash),
    );
    setup.extend_calldata_for_post_v1_execution_info_syscall();
    setup.execute_and_assert();
}

/// Tests `get_execution_info` for data-gas accounts (listed in `data_gas_accounts` in versioned
/// constants). These accounts receive only 2 resource types (L1 gas, L2 gas) instead of 3.
///
/// Only tests Execute mode; the main test covers both modes. This test focuses on data-gas
/// behavior.
#[rstest]
#[cfg_attr(feature = "cairo_native", case(RunnableCairo1::Native))]
#[case(RunnableCairo1::Casm)]
fn test_exclude_l1_data_gas_get_execution_info(
    #[case] runnable: RunnableCairo1,
    #[values(true, false)] only_query: bool,
) {
    let contract = FeatureContract::TestContract(CairoVersion::Cairo1(runnable));
    let flags =
        ExecutionInfoTestFlags { only_query, exclude_l1_data_gas: true, ..Default::default() };
    let class_hash =
        *VersionedConstants::latest_constants().os_constants.data_gas_accounts.first().unwrap();
    let mut setup = create_test_setup(
        contract,
        ExecutionMode::Execute,
        TransactionVersion::THREE,
        &flags,
        Some(class_hash),
    );
    setup.extend_calldata_for_post_v1_execution_info_syscall();
    setup.execute_and_assert();
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
