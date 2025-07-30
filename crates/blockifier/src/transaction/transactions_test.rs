use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock};

use assert_matches::assert_matches;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::{create_calldata, create_trivial_calldata};
use blockifier_test_utils::contracts::FeatureContract;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use expect_test::expect;
use num_bigint::BigUint;
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use rstest_reuse::apply;
use starknet_api::abi::abi_utils::{
    get_fee_token_var_address,
    get_storage_var_address,
    selector_from_name,
};
use starknet_api::abi::constants::CONSTRUCTOR_ENTRY_POINT_NAME;
use starknet_api::block::{FeeType, GasPriceVector};
use starknet_api::contract_class::EntryPointType;
use starknet_api::core::{ascii_as_felt, ClassHash, ContractAddress, Nonce};
use starknet_api::executable_transaction::{
    AccountTransaction as ApiExecutableTransaction,
    DeployAccountTransaction,
    TransactionType,
};
use starknet_api::execution_resources::{GasAmount, GasVector};
use starknet_api::state::StorageKey;
use starknet_api::test_utils::declare::executable_declare_tx;
use starknet_api::test_utils::deploy_account::{
    create_executable_deploy_account_tx_and_update_nonce,
    executable_deploy_account_tx,
    DeployAccountTxArgs,
};
use starknet_api::test_utils::invoke::{executable_invoke_tx, InvokeTxArgs};
use starknet_api::test_utils::{
    NonceManager,
    CHAIN_ID_FOR_TESTS,
    CURRENT_BLOCK_NUMBER,
    CURRENT_BLOCK_NUMBER_FOR_VALIDATE,
    CURRENT_BLOCK_TIMESTAMP,
    CURRENT_BLOCK_TIMESTAMP_FOR_VALIDATE,
    DEFAULT_L1_DATA_GAS_MAX_AMOUNT,
    DEFAULT_L1_GAS_AMOUNT,
    DEFAULT_L2_GAS_MAX_AMOUNT,
    DEFAULT_STRK_L1_DATA_GAS_PRICE,
    DEFAULT_STRK_L1_GAS_PRICE,
    DEFAULT_STRK_L2_GAS_PRICE,
    MAX_FEE,
    TEST_SEQUENCER_ADDRESS,
};
use starknet_api::transaction::fields::Resource::{L1DataGas, L1Gas, L2Gas};
use starknet_api::transaction::fields::{
    AllResourceBounds,
    Calldata,
    Fee,
    GasVectorComputationMode,
    Resource,
    ResourceBounds,
    Tip,
    TransactionSignature,
    ValidResourceBounds,
};
use starknet_api::transaction::{
    constants,
    EventContent,
    EventData,
    EventKey,
    L2ToL1Payload,
    TransactionVersion,
    QUERY_VERSION_BASE,
};
use starknet_api::{
    calldata,
    class_hash,
    contract_address,
    declare_tx_args,
    deploy_account_tx_args,
    felt,
    invoke_tx_args,
    nonce,
};
use starknet_types_core::felt::Felt;

use crate::blockifier_versioned_constants::{AllocationCost, VersionedConstants};
use crate::context::{BlockContext, ChainInfo, FeeTokenAddresses, TransactionContext};
use crate::execution::call_info::{
    CallExecution,
    CallInfo,
    ExecutionSummary,
    MessageToL1,
    OrderedEvent,
    OrderedL2ToL1Message,
    Retdata,
    StorageAccessTracker,
};
use crate::execution::contract_class::TrackedResource;
use crate::execution::entry_point::{CallEntryPoint, CallType};
use crate::execution::errors::{ConstructorEntryPointExecutionError, EntryPointExecutionError};
use crate::execution::stack_trace::{
    Cairo1RevertSummary,
    EntryPointErrorFrame,
    ErrorStack,
    ErrorStackHeader,
    ErrorStackSegment,
};
use crate::execution::syscalls::hint_processor::EmitEventError;
#[cfg(feature = "cairo_native")]
use crate::execution::syscalls::hint_processor::SyscallExecutionError;
#[cfg(feature = "cairo_native")]
use crate::execution::syscalls::vm_syscall_utils::SyscallExecutorBaseError;
use crate::execution::syscalls::vm_syscall_utils::SyscallSelector;
use crate::fee::fee_checks::FeeCheckError;
use crate::fee::fee_utils::{balance_to_big_uint, get_fee_by_gas_vector, GasVectorToL1GasForFee};
use crate::fee::gas_usage::{
    estimate_minimal_gas_vector,
    get_da_gas_cost,
    get_onchain_data_segment_length,
};
use crate::fee::receipt::TransactionReceipt;
use crate::fee::resources::{
    ComputationResources,
    StarknetResources,
    StateResources,
    TransactionResources,
};
use crate::state::cached_state::{CachedState, StateChangesCount, TransactionalState};
use crate::state::errors::StateError;
use crate::state::state_api::{State, StateReader};
use crate::test_utils::contracts::FeatureContractTrait;
use crate::test_utils::dict_state_reader::DictStateReader;
use crate::test_utils::initial_test_state::{fund_account, test_state};
use crate::test_utils::l1_handler::l1handler_tx;
use crate::test_utils::prices::Prices;
use crate::test_utils::test_templates::{cairo_version, two_cairo_versions};
use crate::test_utils::{
    get_const_syscall_resources,
    get_tx_resources,
    test_erc20_sequencer_balance_key,
    SaltManager,
    BALANCE,
};
use crate::transaction::account_transaction::{AccountTransaction, ExecutionFlags};
use crate::transaction::errors::{
    ResourceBoundsError,
    TransactionExecutionError,
    TransactionFeeError,
    TransactionPreValidationError,
};
use crate::transaction::objects::{
    HasRelatedFeeType,
    RevertError,
    TransactionExecutionInfo,
    TransactionInfo,
    TransactionInfoCreator,
};
use crate::transaction::test_utils::{
    block_context,
    calculate_class_info_for_testing,
    create_account_tx_for_validate_test,
    create_account_tx_for_validate_test_nonce_0,
    create_gas_amount_bounds_with_default_price,
    create_test_init_data,
    default_all_resource_bounds,
    default_l1_resource_bounds,
    invoke_tx_with_default_flags,
    l1_resource_bounds,
    versioned_constants,
    FaultyAccountTxCreatorArgs,
    TestInitData,
    CALL_CONTRACT,
    GET_BLOCK_HASH,
    GET_BLOCK_NUMBER,
    GET_BLOCK_TIMESTAMP,
    GET_EXECUTION_INFO,
    GET_SEQUENCER_ADDRESS,
    INVALID,
    VALID,
};
use crate::transaction::transactions::ExecutableTransaction;
use crate::{
    check_tx_execution_error_for_custom_hint,
    check_tx_execution_error_for_invalid_scenario,
    retdata,
};

static DECLARE_REDEPOSIT_AMOUNT: LazyLock<u64> = LazyLock::new(|| {
    let resource_bounds = default_all_resource_bounds();
    let cairo_version = CairoVersion::Cairo1(RunnableCairo1::Casm);
    let block_context = &BlockContext::create_for_account_testing_with_kzg(true);
    let account = FeatureContract::AccountWithoutValidations(cairo_version);
    let empty_contract = FeatureContract::Empty(cairo_version);
    let chain_info = &block_context.chain_info;
    let state = &mut test_state(chain_info, BALANCE, &[(account, 1)]);
    let account_tx = AccountTransaction::new_with_default_flags(executable_declare_tx(
        declare_tx_args! {
            sender_address: account.get_instance_address(0),
            version: TransactionVersion::THREE,
            resource_bounds,
            class_hash: empty_contract.get_class_hash(),
            compiled_class_hash: empty_contract.get_compiled_class_hash(),
            nonce: Nonce(Felt::ZERO),
        },
        calculate_class_info_for_testing(empty_contract.get_class()).clone(),
    ));
    let actual_execution_info = account_tx.execute(state, block_context).unwrap();
    VersionedConstants::latest_constants().os_constants.gas_costs.base.entry_point_initial_budget
        - actual_execution_info.validate_call_info.unwrap().execution.gas_consumed
});
static DEPLOY_ACCOUNT_REDEPOSIT_AMOUNT: LazyLock<u64> = LazyLock::new(|| {
    let block_context = &BlockContext::create_for_account_testing_with_kzg(true);
    let chain_info = &block_context.chain_info;
    let account =
        FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let state = &mut test_state(chain_info, BALANCE, &[(account, 1)]);
    let deploy_account = AccountTransaction::new_with_default_flags(executable_deploy_account_tx(
        deploy_account_tx_args! {
            resource_bounds: default_all_resource_bounds(),
            class_hash: account.get_class_hash(),
        },
    ));
    fund_account(chain_info, deploy_account.tx.contract_address(), BALANCE, &mut state.state);
    let actual_execution_info = deploy_account.execute(state, block_context).unwrap();
    VersionedConstants::latest_constants().os_constants.gas_costs.base.entry_point_initial_budget
        - actual_execution_info.validate_call_info.unwrap().execution.gas_consumed
});
static VERSIONED_CONSTANTS: LazyLock<VersionedConstants> =
    LazyLock::new(VersionedConstants::create_for_testing);

#[fixture]
fn infinite_gas_for_vm_mode() -> u64 {
    VERSIONED_CONSTANTS.infinite_gas_for_vm_mode()
}

#[fixture]
fn versioned_constants_for_account_testing() -> VersionedConstants {
    VERSIONED_CONSTANTS.clone()
}

fn initial_gas_amount_from_block_context(block_context: Option<&BlockContext>) -> GasAmount {
    match block_context {
        Some(block_context) => block_context.versioned_constants.initial_gas_no_user_l2_bound(),
        None => VERSIONED_CONSTANTS.initial_gas_no_user_l2_bound(),
    }
}

struct ExpectedResultTestInvokeTx {
    resources: ExecutionResources,
    validate_gas_consumed: u64,
    execute_gas_consumed: u64,
}

fn user_initial_gas_from_bounds(
    bounds: ValidResourceBounds,
    block_context: Option<&BlockContext>,
) -> GasAmount {
    match bounds {
        ValidResourceBounds::L1Gas(_) => initial_gas_amount_from_block_context(block_context),
        ValidResourceBounds::AllResources(bounds) => bounds.l2_gas.max_amount,
    }
}

#[allow(clippy::too_many_arguments)]
fn expected_validate_call_info(
    class_hash: ClassHash,
    entry_point_selector_name: &str,
    gas_consumed: u64,
    calldata: Calldata,
    storage_address: ContractAddress,
    cairo_version: CairoVersion,
    tracked_resource: TrackedResource,
    user_initial_gas: Option<GasAmount>,
) -> Option<CallInfo> {
    let retdata = match cairo_version {
        CairoVersion::Cairo0 => Retdata::default(),
        CairoVersion::Cairo1(_) => retdata!(*constants::VALIDATE_RETDATA),
    };
    let cairo_native = cairo_version.is_cairo_native();
    // Extra range checks in regular (invoke) validate call, due to calldata passed as array.
    let n_range_checks = match cairo_version {
        CairoVersion::Cairo0 => {
            usize::from(entry_point_selector_name == constants::VALIDATE_ENTRY_POINT_NAME)
        }
        CairoVersion::Cairo1(_) => {
            if entry_point_selector_name == constants::VALIDATE_ENTRY_POINT_NAME { 7 } else { 2 }
        }
    };
    let vm_resources = match tracked_resource {
        TrackedResource::SierraGas => ExecutionResources::default(),
        TrackedResource::CairoSteps => {
            let n_steps = match (entry_point_selector_name, cairo_version) {
                (constants::VALIDATE_DEPLOY_ENTRY_POINT_NAME, CairoVersion::Cairo0) => 13_usize,
                (
                    constants::VALIDATE_DEPLOY_ENTRY_POINT_NAME,
                    CairoVersion::Cairo1(RunnableCairo1::Casm),
                ) => 32_usize,
                (constants::VALIDATE_DECLARE_ENTRY_POINT_NAME, CairoVersion::Cairo0) => 12_usize,
                (
                    constants::VALIDATE_DECLARE_ENTRY_POINT_NAME,
                    CairoVersion::Cairo1(RunnableCairo1::Casm),
                ) => 28_usize,
                (constants::VALIDATE_ENTRY_POINT_NAME, CairoVersion::Cairo0) => 21_usize,
                (
                    constants::VALIDATE_ENTRY_POINT_NAME,
                    CairoVersion::Cairo1(RunnableCairo1::Casm),
                ) => 100_usize,
                (selector, _) => panic!("Selector {selector} is not a known validate selector."),
            };
            ExecutionResources {
                n_steps,
                n_memory_holes: 0,
                builtin_instance_counter: HashMap::from([(
                    BuiltinName::range_check,
                    n_range_checks,
                )]),
            }
            .filter_unused_builtins()
        }
    };
    let initial_gas = match cairo_version {
        CairoVersion::Cairo0 => infinite_gas_for_vm_mode(),
        CairoVersion::Cairo1(_) => match tracked_resource {
            TrackedResource::CairoSteps => initial_gas_amount_from_block_context(None).0,
            TrackedResource::SierraGas => {
                user_initial_gas
                    .unwrap_or(initial_gas_amount_from_block_context(None))
                    .min(VERSIONED_CONSTANTS.os_constants.validate_max_sierra_gas)
                    .0
            }
        },
    };

    Some(CallInfo {
        call: CallEntryPoint {
            class_hash: Some(class_hash),
            code_address: None,
            entry_point_type: EntryPointType::External,
            entry_point_selector: selector_from_name(entry_point_selector_name),
            calldata,
            storage_address,
            caller_address: ContractAddress::default(),
            call_type: CallType::Call,
            initial_gas,
        },
        // The account contract we use for testing has trivial `validate` functions.
        resources: vm_resources,
        execution: CallExecution { retdata, gas_consumed, cairo_native, ..Default::default() },
        tracked_resource,
        builtin_counters: HashMap::from([(BuiltinName::range_check, n_range_checks)])
            .into_iter()
            .filter(|builtin| builtin.1 > 0)
            .collect(),
        ..Default::default()
    })
}

fn expected_fee_transfer_call_info(
    tx_context: &TransactionContext,
    account_address: ContractAddress,
    actual_fee: Fee,
    expected_fee_token_class_hash: ClassHash,
    cairo_version: CairoVersion,
) -> Option<CallInfo> {
    let block_context = &tx_context.block_context;
    let fee_type = &tx_context.tx_info.fee_type();
    let expected_sequencer_address = block_context.block_info.sequencer_address;
    let expected_sequencer_address_felt = *expected_sequencer_address.0.key();
    // The least significant 128 bits of the expected amount transferred.
    let lsb_expected_amount = felt!(actual_fee.0);
    // The most significant 128 bits of the expected amount transferred.
    let msb_expected_amount = felt!(0_u8);
    let storage_address = block_context.chain_info.fee_token_address(fee_type);
    let expected_fee_transfer_call = CallEntryPoint {
        class_hash: Some(expected_fee_token_class_hash),
        code_address: None,
        entry_point_type: EntryPointType::External,
        entry_point_selector: selector_from_name(constants::TRANSFER_ENTRY_POINT_NAME),
        calldata: calldata![
            expected_sequencer_address_felt, // Recipient.
            lsb_expected_amount,
            msb_expected_amount
        ],
        storage_address,
        caller_address: account_address,
        call_type: CallType::Call,
        initial_gas: block_context
            .versioned_constants
            .os_constants
            .gas_costs
            .base
            .default_initial_gas_cost,
    };
    let expected_fee_sender_address = *account_address.0.key();
    let expected_fee_transfer_event = OrderedEvent {
        order: 0,
        event: EventContent {
            keys: vec![EventKey(selector_from_name(constants::TRANSFER_EVENT_NAME).0)],
            data: EventData(vec![
                expected_fee_sender_address,
                expected_sequencer_address_felt, // Recipient.
                lsb_expected_amount,
                msb_expected_amount,
            ]),
        },
    };

    let sender_balance_key_low = get_fee_token_var_address(account_address);
    let sender_balance_key_high =
        sender_balance_key_low.next_storage_key().expect("Cannot get sender balance high key.");
    let sequencer_balance_key_low = get_fee_token_var_address(expected_sequencer_address);
    let sequencer_balance_key_high = sequencer_balance_key_low
        .next_storage_key()
        .expect("Cannot get sequencer balance high key.");
    let cairo_native = cairo_version.is_cairo_native();
    let builtin_counters = match cairo_version {
        CairoVersion::Cairo0 => {
            HashMap::from([(BuiltinName::range_check, 32), (BuiltinName::pedersen, 4)])
        }
        CairoVersion::Cairo1(_) => {
            HashMap::from([(BuiltinName::range_check, 38), (BuiltinName::pedersen, 4)])
        }
    };
    let expected_tracked_resource = match cairo_version {
        CairoVersion::Cairo0 => TrackedResource::CairoSteps,
        CairoVersion::Cairo1(_) => TrackedResource::SierraGas,
    };
    let expected_gas_consumed = match cairo_version {
        CairoVersion::Cairo0 => 0_u64,
        CairoVersion::Cairo1(_) => 158310_u64,
    };
    let expected_resources = match cairo_version {
        CairoVersion::Cairo0 => Prices::FeeTransfer(account_address, *fee_type).into(),
        CairoVersion::Cairo1(_) => ExecutionResources::default(),
    };
    Some(CallInfo {
        call: expected_fee_transfer_call,
        execution: CallExecution {
            retdata: retdata![felt!(constants::FELT_TRUE)],
            events: vec![expected_fee_transfer_event],
            cairo_native,
            gas_consumed: expected_gas_consumed,
            ..Default::default()
        },
        resources: expected_resources,
        // We read sender and recipient balance - Uint256(BALANCE, 0) then Uint256(0, 0).
        storage_access_tracker: StorageAccessTracker {
            storage_read_values: vec![felt!(BALANCE.0), felt!(0_u8), felt!(0_u8), felt!(0_u8)],
            accessed_storage_keys: HashSet::from_iter(vec![
                sender_balance_key_low,
                sender_balance_key_high,
                sequencer_balance_key_low,
                sequencer_balance_key_high,
            ]),
            ..Default::default()
        },
        tracked_resource: expected_tracked_resource,
        builtin_counters,
        ..Default::default()
    })
}

fn get_expected_cairo_resources(
    versioned_constants: &VersionedConstants,
    tx_type: TransactionType,
    starknet_resources: &StarknetResources,
    call_infos: Vec<&Option<CallInfo>>,
) -> (ExecutionResources, ExecutionResources) {
    let expected_os_cairo_resources =
        versioned_constants.get_additional_os_tx_resources(tx_type, starknet_resources, false);
    let mut expected_tx_cairo_resources = ExecutionResources::default();
    for call_info in call_infos {
        if let Some(call_info) = &call_info {
            expected_tx_cairo_resources += &call_info.resources
        };
    }

    (expected_tx_cairo_resources, expected_os_cairo_resources)
}

/// Given the fee result of a single account transaction, verifies the final balances of the account
/// and the sequencer (in both fee types) are as expected (assuming the initial sequencer balances
/// are zero).
fn validate_final_balances(
    state: &mut CachedState<DictStateReader>,
    chain_info: &ChainInfo,
    expected_actual_fee: Fee,
    erc20_account_balance_key: StorageKey,
    fee_type: &FeeType,
    initial_account_balance_eth: Fee,
    initial_account_balance_strk: Fee,
) {
    // Expected balances of account and sequencer, per fee type.
    let (expected_sequencer_balance_eth, expected_sequencer_balance_strk) = match fee_type {
        FeeType::Eth => (felt!(expected_actual_fee.0), Felt::ZERO),
        FeeType::Strk => (Felt::ZERO, felt!(expected_actual_fee.0)),
    };
    let mut expected_account_balance_eth = initial_account_balance_eth.0;
    let mut expected_account_balance_strk = initial_account_balance_strk.0;
    if fee_type == &FeeType::Eth {
        expected_account_balance_eth -= expected_actual_fee.0;
    } else {
        expected_account_balance_strk -= expected_actual_fee.0;
    }

    // Verify balances of both accounts, of both fee types, are as expected.
    let FeeTokenAddresses { eth_fee_token_address, strk_fee_token_address } =
        chain_info.fee_token_addresses;
    for (fee_address, expected_account_balance, expected_sequencer_balance) in [
        (eth_fee_token_address, expected_account_balance_eth, expected_sequencer_balance_eth),
        (strk_fee_token_address, expected_account_balance_strk, expected_sequencer_balance_strk),
    ] {
        let account_balance = state.get_storage_at(fee_address, erc20_account_balance_key).unwrap();
        assert_eq!(account_balance, felt!(expected_account_balance));
        assert_eq!(
            state.get_storage_at(fee_address, test_erc20_sequencer_balance_key()).unwrap(),
            expected_sequencer_balance
        );
    }
}

fn add_kzg_da_resources_to_resources_mapping(
    target: &mut ExecutionResources,
    state_changes_count: &StateChangesCount,
    versioned_constants: &VersionedConstants,
    use_kzg_da: bool,
) {
    if !use_kzg_da {
        return;
    }

    let data_segment_length = get_onchain_data_segment_length(state_changes_count);
    let os_kzg_da_resources = versioned_constants.os_kzg_da_resources(data_segment_length);

    target.n_steps += os_kzg_da_resources.n_steps;

    os_kzg_da_resources.builtin_instance_counter.into_iter().for_each(|(key, value)| {
        target.builtin_instance_counter.entry(key).and_modify(|v| *v += value).or_insert(value);
    });
}

// TODO(Dori): Add a test case that test cairo1 contract that uses VM resources.
#[rstest]
#[case::with_cairo0_account(
    ExpectedResultTestInvokeTx{
        resources: &get_const_syscall_resources(SyscallSelector::CallContract) + &ExecutionResources {
            n_steps: 62,
            n_memory_holes:  0,
            builtin_instance_counter: HashMap::from([(BuiltinName::range_check, 1)]),
        },
        validate_gas_consumed: 0,
        execute_gas_consumed: 0,
    },
    CairoVersion::Cairo0)]
#[case::with_cairo1_account(
    ExpectedResultTestInvokeTx{
        resources: ExecutionResources::default(),
        validate_gas_consumed: 8990, // The gas consumption results from parsing the input
            // arguments.
        execute_gas_consumed: 115190,
    },
    CairoVersion::Cairo1(RunnableCairo1::Casm))]
#[cfg_attr(feature = "cairo_native", case::with_cairo1_native_account(
    ExpectedResultTestInvokeTx{
        resources: ExecutionResources::default(),
        validate_gas_consumed: 8990, // The gas consumption results from parsing the input
            // arguments.
        execute_gas_consumed: 115190,
    },
    CairoVersion::Cairo1(RunnableCairo1::Native)))]
// TODO(Tzahi): Add calls to cairo1 test contracts (where gas flows to and from the inner call).
fn test_invoke_tx(
    #[values(default_l1_resource_bounds(), default_all_resource_bounds())]
    resource_bounds: ValidResourceBounds,
    #[case] mut expected_arguments: ExpectedResultTestInvokeTx,
    #[case] account_cairo_version: CairoVersion,
    #[values(false, true)] use_kzg_da: bool,
) {
    let block_context = &BlockContext::create_for_account_testing_with_kzg(use_kzg_da);
    let versioned_constants = &block_context.versioned_constants;
    let account_contract = FeatureContract::AccountWithoutValidations(account_cairo_version);
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo0);
    let chain_info = &block_context.chain_info;
    let state = &mut test_state(chain_info, BALANCE, &[(account_contract, 1), (test_contract, 1)]);
    let test_contract_address = test_contract.get_instance_address(0);
    let account_contract_address = account_contract.get_instance_address(0);
    let calldata = create_trivial_calldata(test_contract_address);
    let cairo_native = account_cairo_version.is_cairo_native();
    let invoke_tx = invoke_tx_with_default_flags(invoke_tx_args! {
        sender_address: account_contract_address,
        calldata: Calldata(Arc::clone(&calldata.0)),
        resource_bounds,
    });

    // Extract invoke transaction fields for testing, as it is consumed when creating an account
    // transaction.
    let calldata_length = invoke_tx.calldata_length();
    let signature_length = invoke_tx.signature_length();
    let state_changes_for_fee = StateChangesCount {
        n_storage_updates: 1,
        n_modified_contracts: 1,
        ..StateChangesCount::default()
    };
    let starknet_resources = StarknetResources::new(
        calldata_length,
        signature_length,
        0,
        StateResources::new_for_testing(state_changes_for_fee, 0),
        None,
        ExecutionSummary::default(),
    );
    let sender_address = invoke_tx.sender_address();

    let tx_context = block_context.to_tx_context(&invoke_tx);

    let actual_execution_info = invoke_tx.execute(state, block_context).unwrap();

    let tracked_resource = account_contract
        .get_runnable_class()
        .tracked_resource(&versioned_constants.min_sierra_version_for_sierra_gas, None);

    // Build expected validate call info.
    let expected_account_class_hash = account_contract.get_class_hash();
    let initial_gas = user_initial_gas_from_bounds(resource_bounds, Some(block_context));
    let expected_validate_call_info = expected_validate_call_info(
        expected_account_class_hash,
        constants::VALIDATE_ENTRY_POINT_NAME,
        expected_arguments.validate_gas_consumed,
        calldata,
        sender_address,
        account_cairo_version,
        tracked_resource,
        Some(initial_gas.min(versioned_constants.os_constants.validate_max_sierra_gas)),
    );

    // Build expected execute call info.
    let expected_return_result_calldata = vec![felt!(2_u8)];

    let expected_validated_call = expected_validate_call_info.as_ref().unwrap().call.clone();
    let expected_initial_execution_gas = versioned_constants
        .os_constants
        .execute_max_sierra_gas
        .min(initial_gas - GasAmount(expected_arguments.validate_gas_consumed))
        .0;
    let expected_execute_call = CallEntryPoint {
        entry_point_selector: selector_from_name(constants::EXECUTE_ENTRY_POINT_NAME),
        initial_gas: match account_cairo_version {
            CairoVersion::Cairo0 => versioned_constants.infinite_gas_for_vm_mode(),
            CairoVersion::Cairo1(_) => expected_initial_execution_gas,
        },
        ..expected_validated_call
    };

    let expected_inner_call_vm_resources =
        ExecutionResources { n_steps: 23, n_memory_holes: 0, ..Default::default() };

    let expected_return_result_call = CallEntryPoint {
        entry_point_selector: selector_from_name("return_result"),
        class_hash: Some(test_contract.get_class_hash()),
        code_address: Some(test_contract_address),
        entry_point_type: EntryPointType::External,
        calldata: Calldata(expected_return_result_calldata.clone().into()),
        storage_address: test_contract_address,
        caller_address: sender_address,
        call_type: CallType::Call,
        initial_gas: versioned_constants.infinite_gas_for_vm_mode(),
    };

    let expected_return_result_retdata = Retdata(expected_return_result_calldata);
    let expected_inner_calls = vec![CallInfo {
        call: expected_return_result_call,
        execution: CallExecution {
            retdata: expected_return_result_retdata.clone(),
            ..Default::default()
        },
        resources: expected_inner_call_vm_resources.clone(),
        ..Default::default()
    }];
    let (expected_validate_gas_for_fee, expected_execute_gas_for_fee) = match tracked_resource {
        TrackedResource::CairoSteps => (GasAmount::default(), GasAmount::default()),
        TrackedResource::SierraGas => {
            expected_arguments.resources = expected_inner_call_vm_resources;
            (
                expected_arguments.validate_gas_consumed.into(),
                expected_arguments.execute_gas_consumed.into(),
            )
        }
    };
    let builtin_counters = match account_cairo_version {
        CairoVersion::Cairo0 => HashMap::from([(BuiltinName::range_check, 19)]),
        CairoVersion::Cairo1(_) => HashMap::from([(BuiltinName::range_check, 27)]),
    };
    let expected_execute_call_info = Some(CallInfo {
        call: expected_execute_call,
        execution: CallExecution {
            retdata: Retdata(expected_return_result_retdata.0),
            gas_consumed: expected_arguments.execute_gas_consumed,
            cairo_native,
            ..Default::default()
        },
        resources: expected_arguments.resources,
        inner_calls: expected_inner_calls,
        tracked_resource,
        builtin_counters,
        ..Default::default()
    });

    // Build expected fee transfer call info.
    let fee_type = &tx_context.tx_info.fee_type();
    let expected_actual_fee = actual_execution_info.receipt.fee;
    let expected_fee_transfer_call_info = expected_fee_transfer_call_info(
        &tx_context,
        sender_address,
        expected_actual_fee,
        FeatureContract::ERC20(account_cairo_version).get_class_hash(),
        account_cairo_version,
    );

    let da_gas = starknet_resources.state.da_gas_vector(use_kzg_da);

    let (expected_tx_cairo_resources, expected_os_cairo_resources) = get_expected_cairo_resources(
        versioned_constants,
        TransactionType::InvokeFunction,
        &starknet_resources,
        vec![&expected_validate_call_info, &expected_execute_call_info],
    );

    let mut expected_actual_resources = TransactionResources {
        starknet_resources,
        computation: ComputationResources {
            tx_vm_resources: expected_tx_cairo_resources,
            os_vm_resources: expected_os_cairo_resources,
            sierra_gas: expected_validate_gas_for_fee + expected_execute_gas_for_fee,
            ..Default::default()
        },
    };

    add_kzg_da_resources_to_resources_mapping(
        &mut expected_actual_resources.computation.os_vm_resources,
        &state_changes_for_fee,
        versioned_constants,
        use_kzg_da,
    );

    let total_gas = expected_actual_resources.to_gas_vector(
        &block_context.versioned_constants,
        block_context.block_info.use_kzg_da,
        &resource_bounds.get_gas_vector_computation_mode(),
    );

    let expected_execution_info = TransactionExecutionInfo {
        validate_call_info: expected_validate_call_info,
        execute_call_info: expected_execute_call_info,
        fee_transfer_call_info: expected_fee_transfer_call_info,
        receipt: TransactionReceipt {
            fee: expected_actual_fee,
            da_gas,
            resources: expected_actual_resources,
            gas: total_gas,
        },
        revert_error: None,
    };

    // Test execution info result.
    assert_eq!(actual_execution_info, expected_execution_info);

    // Test nonce update.
    let nonce_from_state = state.get_nonce_at(sender_address).unwrap();
    assert_eq!(nonce_from_state, nonce!(1_u8));

    // Test final balances.
    validate_final_balances(
        state,
        chain_info,
        expected_actual_fee,
        get_fee_token_var_address(account_contract_address),
        fee_type,
        BALANCE,
        BALANCE,
    );
}

// Verifies the storage after each invoke execution in test_invoke_tx_advanced_operations.
fn verify_storage_after_invoke_advanced_operations(
    state: &mut CachedState<DictStateReader>,
    contract_address: ContractAddress,
    account_address: ContractAddress,
    index: Felt,
    expected_counters: [Felt; 2],
    expected_ec_point: [Felt; 2],
    expected_nonce: Nonce,
) {
    // Verify the two_counters values in storage.
    let key = get_storage_var_address("two_counters", &[index]);
    let value = state.get_storage_at(contract_address, key).unwrap();
    assert_eq!(value, expected_counters[0]);
    let key = key.next_storage_key().unwrap();
    let value = state.get_storage_at(contract_address, key).unwrap();
    assert_eq!(value, expected_counters[1]);

    // Verify the ec_point values in storage.
    let key = get_storage_var_address("ec_point", &[]);
    let value = state.get_storage_at(contract_address, key).unwrap();
    assert_eq!(value, expected_ec_point[0]);
    let key = key.next_storage_key().unwrap();
    let value = state.get_storage_at(contract_address, key).unwrap();
    assert_eq!(value, expected_ec_point[1]);

    // Verify the nonce value in storage.
    let nonce_from_state = state.get_nonce_at(account_address).unwrap();
    assert_eq!(nonce_from_state, expected_nonce);
}

#[rstest]
#[case::with_cairo0_account(CairoVersion::Cairo0)]
#[case::with_cairo1_account(CairoVersion::Cairo1(RunnableCairo1::Casm))]
#[cfg_attr(
    feature = "cairo_native",
    case::with_cairo1_native_account(CairoVersion::Cairo1(RunnableCairo1::Native))
)]
fn test_invoke_tx_advanced_operations(
    block_context: BlockContext,
    default_all_resource_bounds: ValidResourceBounds,
    #[case] cairo_version: CairoVersion,
) {
    let block_context = &block_context;
    let TestInitData { mut state, account_address, contract_address, mut nonce_manager } =
        create_test_init_data(&block_context.chain_info, cairo_version);
    let index = felt!(123_u32);
    let base_tx_args = invoke_tx_args! {
        resource_bounds: default_all_resource_bounds,
        sender_address: account_address,
    };

    // Invoke advance_counter function.
    let counter_diffs = [101_u32, 102_u32];
    let initial_counters = [felt!(counter_diffs[0]), felt!(counter_diffs[1])];
    let calldata_args = vec![index, initial_counters[0], initial_counters[1]];

    let account_tx = invoke_tx_with_default_flags(invoke_tx_args! {
        nonce: nonce_manager.next(account_address),
        calldata:
            create_calldata(contract_address, "advance_counter", &calldata_args),
        ..base_tx_args.clone()
    });
    account_tx.execute(&mut state, block_context).unwrap();

    let next_nonce = nonce_manager.next(account_address);
    let initial_ec_point = [Felt::ZERO, Felt::ZERO];
    verify_storage_after_invoke_advanced_operations(
        &mut state,
        contract_address,
        account_address,
        index,
        initial_counters,
        initial_ec_point,
        next_nonce,
    );

    // Invoke call_xor_counters function.
    let xor_values = [31_u32, 32_u32];
    let calldata_args =
        vec![*contract_address.0.key(), index, felt!(xor_values[0]), felt!(xor_values[1])];

    let account_tx = invoke_tx_with_default_flags(invoke_tx_args! {
        nonce: next_nonce,
        calldata:
            create_calldata(contract_address, "call_xor_counters", &calldata_args),
        ..base_tx_args.clone()
    });
    account_tx.execute(&mut state, block_context).unwrap();

    let expected_counters =
        [felt!(counter_diffs[0] ^ xor_values[0]), felt!(counter_diffs[1] ^ xor_values[1])];
    let next_nonce = nonce_manager.next(account_address);
    verify_storage_after_invoke_advanced_operations(
        &mut state,
        contract_address,
        account_address,
        index,
        expected_counters,
        initial_ec_point,
        next_nonce,
    );

    // Invoke test_ec_op function.
    let account_tx = invoke_tx_with_default_flags(invoke_tx_args! {
        nonce: next_nonce,
        calldata:
            create_calldata(contract_address, "test_ec_op", &[]),
        ..base_tx_args.clone()
    });
    account_tx.execute(&mut state, block_context).unwrap();

    let expected_ec_point = [
        Felt::from_bytes_be(&[
            0x05_u8, 0x07_u8, 0xF8_u8, 0x28_u8, 0xEA_u8, 0xE0_u8, 0x0C_u8, 0x08_u8, 0xED_u8,
            0x10_u8, 0x60_u8, 0x5B_u8, 0xAA_u8, 0xD4_u8, 0x80_u8, 0xB7_u8, 0x4B_u8, 0x0E_u8,
            0x9B_u8, 0x61_u8, 0x9C_u8, 0x1A_u8, 0x2C_u8, 0x53_u8, 0xFB_u8, 0x75_u8, 0x86_u8,
            0xE3_u8, 0xEE_u8, 0x1A_u8, 0x82_u8, 0xBA_u8,
        ]),
        Felt::from_bytes_be(&[
            0x05_u8, 0x43_u8, 0x9A_u8, 0x5D_u8, 0xC0_u8, 0x8C_u8, 0xC1_u8, 0x35_u8, 0x64_u8,
            0x11_u8, 0xA4_u8, 0x57_u8, 0x8F_u8, 0x50_u8, 0x71_u8, 0x54_u8, 0xB4_u8, 0x84_u8,
            0x7B_u8, 0xAA_u8, 0x73_u8, 0x70_u8, 0x68_u8, 0x17_u8, 0x1D_u8, 0xFA_u8, 0x6C_u8,
            0x8A_u8, 0xB3_u8, 0x49_u8, 0x9D_u8, 0x8B_u8,
        ]),
    ];
    let next_nonce = nonce_manager.next(account_address);
    verify_storage_after_invoke_advanced_operations(
        &mut state,
        contract_address,
        account_address,
        index,
        expected_counters,
        expected_ec_point,
        next_nonce,
    );

    // Invoke add_signature_to_counters function.
    let signature_values = [Felt::from(200_u64), Felt::from(300_u64)];
    let signature = TransactionSignature(signature_values.to_vec().into());

    let account_tx = invoke_tx_with_default_flags(invoke_tx_args! {
        signature,
        nonce: next_nonce,
        calldata:
            create_calldata(contract_address, "add_signature_to_counters", &[index]),
        ..base_tx_args.clone()
    });
    account_tx.execute(&mut state, block_context).unwrap();

    let expected_counters = [
        (expected_counters[0] + signature_values[0]),
        (expected_counters[1] + signature_values[1]),
    ];
    let next_nonce = nonce_manager.next(account_address);
    verify_storage_after_invoke_advanced_operations(
        &mut state,
        contract_address,
        account_address,
        index,
        expected_counters,
        expected_ec_point,
        next_nonce,
    );

    // Invoke send_message function that send a message to L1.
    let to_address = Felt::from(85);
    let account_tx = invoke_tx_with_default_flags(invoke_tx_args! {
        nonce: next_nonce,
        calldata:
            create_calldata(contract_address, "send_message", &[to_address]),
        ..base_tx_args
    });
    let execution_info = account_tx.execute(&mut state, block_context).unwrap();
    let next_nonce = nonce_manager.next(account_address);
    verify_storage_after_invoke_advanced_operations(
        &mut state,
        contract_address,
        account_address,
        index,
        expected_counters,
        expected_ec_point,
        next_nonce,
    );
    let expected_msg = OrderedL2ToL1Message {
        order: 0,
        message: MessageToL1 {
            to_address: to_address.into(),
            payload: L2ToL1Payload(vec![felt!(12_u32), felt!(34_u32)]),
        },
    };
    assert_eq!(
        expected_msg,
        execution_info.execute_call_info.unwrap().inner_calls[0].execution.l2_to_l1_messages[0]
    );
}

#[apply(cairo_version)]
#[case(TransactionVersion::ONE, FeeType::Eth)]
#[case(TransactionVersion::THREE, FeeType::Strk)]
fn test_state_get_fee_token_balance(
    block_context: BlockContext,
    #[case] tx_version: TransactionVersion,
    #[case] fee_type: FeeType,
    cairo_version: CairoVersion,
) {
    let TestInitData { mut state, account_address, .. } =
        create_test_init_data(&block_context.chain_info, cairo_version);
    let (mint_high, mint_low) = (felt!(54_u8), felt!(39_u8));
    let recipient_int = 10_u8;
    let recipient = felt!(recipient_int);
    let fee_token_address = block_context.chain_info.fee_token_address(&fee_type);

    // Give the account mint privileges.
    state
        .set_storage_at(
            fee_token_address,
            get_storage_var_address("permitted_minter", &[]),
            *account_address.0.key(),
        )
        .unwrap();

    // Mint some tokens.
    let execute_calldata =
        create_calldata(fee_token_address, "permissionedMint", &[recipient, mint_low, mint_high]);
    let account_tx = invoke_tx_with_default_flags(invoke_tx_args! {
        max_fee: MAX_FEE,
        resource_bounds: default_l1_resource_bounds(),
        sender_address: account_address,
        calldata: execute_calldata,
        version: tx_version,
        nonce: Nonce::default(),
    });
    account_tx.execute(&mut state, &block_context).unwrap();

    // Get balance from state, and validate.
    let (low, high) =
        state.get_fee_token_balance(contract_address!(recipient_int), fee_token_address).unwrap();

    assert_eq!(low, mint_low);
    assert_eq!(high, mint_high);
}

fn assert_resource_bounds_exceed_balance_failure(
    state: &mut CachedState<DictStateReader>,
    block_context: &BlockContext,
    invalid_tx: AccountTransaction,
) {
    let tx_error = invalid_tx.execute(state, block_context).unwrap_err();
    match invalid_tx.create_tx_info() {
        TransactionInfo::Deprecated(context) => {
            assert_matches!(
                tx_error,
                TransactionExecutionError::TransactionPreValidationError(boxed_error)
                => assert_matches!(
                    *boxed_error,
                    TransactionPreValidationError::TransactionFeeError(boxed_fee_error)
                    if matches!(
                        *boxed_fee_error,
                        TransactionFeeError::MaxFeeExceedsBalance{ max_fee, .. }
                        if max_fee == context.max_fee
                    )
                )
            );
        }
        TransactionInfo::Current(context) => match context.resource_bounds {
            ValidResourceBounds::L1Gas(l1_bounds) => assert_matches!(
                tx_error,
                TransactionExecutionError::TransactionPreValidationError(boxed_error)
                => assert_matches!(
                    *boxed_error,
                    TransactionPreValidationError::TransactionFeeError(boxed_fee_error)
                    if matches!(
                        *boxed_fee_error,
                        TransactionFeeError::GasBoundsExceedBalance{
                            resource, max_amount, max_price, ..
                        }
                        if max_amount == l1_bounds.max_amount
                            && max_price == l1_bounds.max_price_per_unit
                            && resource == L1Gas
                    )
                )
            ),
            ValidResourceBounds::AllResources(actual_bounds) => {
                assert_matches!(
                    tx_error,
                    TransactionExecutionError::TransactionPreValidationError(boxed_error)
                    => assert_matches!(
                        *boxed_error,
                        TransactionPreValidationError::TransactionFeeError(boxed_fee_error)
                        if matches!(
                            *boxed_fee_error,
                            TransactionFeeError::ResourcesBoundsExceedBalance {
                                bounds: error_bounds, ..
                            }
                            if actual_bounds == error_bounds
                        )
                    )
                );
            }
        },
    };
}

#[rstest]
#[case::with_cairo0_account(CairoVersion::Cairo0)]
#[case::with_cairo1_account(CairoVersion::Cairo1(RunnableCairo1::Casm))]
#[cfg_attr(
    feature = "cairo_native",
    case::with_cairo1_native_account(CairoVersion::Cairo1(RunnableCairo1::Native))
)]
fn test_estimate_minimal_gas_vector(
    mut block_context: BlockContext,
    #[values(true, false)] use_kzg_da: bool,
    #[values(GasVectorComputationMode::NoL2Gas, GasVectorComputationMode::All)]
    gas_vector_computation_mode: GasVectorComputationMode,
    #[case] cairo_version: CairoVersion,
) {
    block_context.block_info.use_kzg_da = use_kzg_da;
    let TestInitData { account_address, contract_address, .. } =
        create_test_init_data(&block_context.chain_info, cairo_version);

    let valid_invoke_tx_args = invoke_tx_args! {
        sender_address: account_address,
        calldata: create_trivial_calldata(contract_address),
        max_fee: MAX_FEE
    };

    // The minimal gas estimate does not depend on tx version.
    let tx = &invoke_tx_with_default_flags(valid_invoke_tx_args);
    let minimal_gas_vector =
        estimate_minimal_gas_vector(&block_context, tx, &gas_vector_computation_mode);
    let minimal_l1_gas = minimal_gas_vector.l1_gas;
    let minimal_l2_gas = minimal_gas_vector.l2_gas;
    let minimal_l1_data_gas = minimal_gas_vector.l1_data_gas;
    if gas_vector_computation_mode == GasVectorComputationMode::NoL2Gas || !use_kzg_da {
        assert!(minimal_l1_gas > 0_u8.into());
    }
    assert_eq!(
        minimal_l2_gas > 0_u8.into(),
        gas_vector_computation_mode == GasVectorComputationMode::All
    );
    assert_eq!(minimal_l1_data_gas > 0_u8.into(), use_kzg_da);
}

#[rstest]
#[case::with_cairo0_account(CairoVersion::Cairo0)]
#[case::with_cairo1_account(CairoVersion::Cairo1(RunnableCairo1::Casm))]
#[cfg_attr(
    feature = "cairo_native",
    case::with_cairo1_native_account(CairoVersion::Cairo1(RunnableCairo1::Native))
)]
fn test_max_fee_exceeds_balance(
    mut block_context: BlockContext,
    #[values(default_l1_resource_bounds(), default_all_resource_bounds())]
    resource_bounds: ValidResourceBounds,
    #[values(true, false)] use_kzg_da: bool,
    #[case] cairo_version: CairoVersion,
) {
    block_context.block_info.use_kzg_da = use_kzg_da;
    let TestInitData { mut state, account_address, contract_address, .. } =
        create_test_init_data(&block_context.chain_info, cairo_version);
    let default_invoke_args = invoke_tx_args! {
        sender_address: account_address,
        calldata: create_trivial_calldata(contract_address)
    };

    // Deploy.
    let invalid_tx = AccountTransaction::new_with_default_flags(executable_deploy_account_tx(
        deploy_account_tx_args! {
            resource_bounds,
            class_hash: FeatureContract::TestContract(cairo_version).get_class_hash()
        },
    ));
    assert_resource_bounds_exceed_balance_failure(&mut state, &block_context, invalid_tx);

    // V1 Invoke.
    let invalid_max_fee = Fee(BALANCE.0 + 1);
    let invalid_tx = invoke_tx_with_default_flags(invoke_tx_args! {
        max_fee: invalid_max_fee,
        version: TransactionVersion::ONE,
        ..default_invoke_args.clone()
    });
    assert_resource_bounds_exceed_balance_failure(&mut state, &block_context, invalid_tx);

    // V3 txs.
    macro_rules! assert_resource_overdraft {
        ($invalid_resource_bounds:expr) => {
            // V3 invoke.
            let invalid_tx = invoke_tx_with_default_flags(invoke_tx_args! {
                resource_bounds: $invalid_resource_bounds,
                ..default_invoke_args.clone()
            });
            assert_resource_bounds_exceed_balance_failure(&mut state, &block_context, invalid_tx);
            // Declare.
            let contract_to_declare =
                FeatureContract::Empty(CairoVersion::Cairo1(RunnableCairo1::Casm));
            let class_info = calculate_class_info_for_testing(contract_to_declare.get_class());
            let invalid_tx = AccountTransaction::new_with_default_flags(executable_declare_tx(
                declare_tx_args! {
                    class_hash: contract_to_declare.get_class_hash(),
                    compiled_class_hash: contract_to_declare.get_compiled_class_hash(),
                    sender_address: account_address,
                    resource_bounds: $invalid_resource_bounds,
                },
                class_info,
            ));
            assert_resource_bounds_exceed_balance_failure(&mut state, &block_context, invalid_tx);
        };
    }

    // Test V3 insufficient balance w.r.t. the bounds type.
    match resource_bounds.get_gas_vector_computation_mode() {
        GasVectorComputationMode::NoL2Gas => {
            let balance_over_l1_gas_price = BALANCE.checked_div(DEFAULT_STRK_L1_GAS_PRICE).unwrap();
            let invalid_resource_bounds = l1_resource_bounds(
                (balance_over_l1_gas_price.0 + 1).into(),
                DEFAULT_STRK_L1_GAS_PRICE.into(),
            );
            assert_resource_overdraft!(invalid_resource_bounds);
        }
        GasVectorComputationMode::All => {
            // Divide balance into 3 parts, one for each resource. Get overdraft on each.
            let partial_balance = Fee(BALANCE.0 / 3);
            let l1_gas_amount = partial_balance.checked_div(DEFAULT_STRK_L1_GAS_PRICE).unwrap();
            let l2_gas_amount = partial_balance.checked_div(DEFAULT_STRK_L2_GAS_PRICE).unwrap();
            let l1_data_gas_amount =
                partial_balance.checked_div(DEFAULT_STRK_L1_DATA_GAS_PRICE).unwrap();
            let ValidResourceBounds::AllResources(mut base_resource_bounds) =
                create_gas_amount_bounds_with_default_price(GasVector {
                    l1_gas: l1_gas_amount,
                    l2_gas: l2_gas_amount,
                    l1_data_gas: l1_data_gas_amount,
                })
            else {
                panic!("Invalid resource bounds.");
            };
            // L1 gas overdraft.
            base_resource_bounds.l1_gas.max_amount.0 += 10;
            assert_resource_overdraft!(ValidResourceBounds::AllResources(base_resource_bounds));
            base_resource_bounds.l1_gas.max_amount.0 -= 10;
            // L2 gas overdraft.
            base_resource_bounds.l2_gas.max_amount.0 += 10;
            assert_resource_overdraft!(ValidResourceBounds::AllResources(base_resource_bounds));
            base_resource_bounds.l2_gas.max_amount.0 -= 10;
            // L1 data gas overdraft.
            base_resource_bounds.l1_data_gas.max_amount.0 += 10;
            assert_resource_overdraft!(ValidResourceBounds::AllResources(base_resource_bounds));
            base_resource_bounds.l1_data_gas.max_amount.0 -= 10;
            // Tip yields l2 gas overdraft.
            let invalid_tx = invoke_tx_with_default_flags(invoke_tx_args! {
                resource_bounds: ValidResourceBounds::AllResources(base_resource_bounds),
                tip: Tip(1_u64),
                ..default_invoke_args.clone()
            });
            assert_resource_bounds_exceed_balance_failure(&mut state, &block_context, invalid_tx);
        }
    }
}

#[rstest]
#[case::with_cairo0_account(CairoVersion::Cairo0)]
#[case::with_cairo1_account(CairoVersion::Cairo1(RunnableCairo1::Casm))]
#[cfg_attr(
    feature = "cairo_native",
    case::with_cairo1_native_account(CairoVersion::Cairo1(RunnableCairo1::Native))
)]
fn test_insufficient_new_resource_bounds_pre_validation(
    mut block_context: BlockContext,
    #[values(true, false)] use_kzg_da: bool,
    #[case] cairo_version: CairoVersion,
) {
    block_context.block_info.use_kzg_da = use_kzg_da;
    let TestInitData { mut state, account_address, contract_address, .. } =
        create_test_init_data(&block_context.chain_info, cairo_version);

    let valid_invoke_tx_args = invoke_tx_args! {
        sender_address: account_address,
        calldata: create_trivial_calldata(contract_address),
        max_fee: MAX_FEE
    };
    let tx = &invoke_tx_with_default_flags(valid_invoke_tx_args.clone());

    // V3 transaction.
    let GasPriceVector {
        l1_gas_price: actual_strk_l1_gas_price,
        l1_data_gas_price: actual_strk_l1_data_gas_price,
        l2_gas_price: actual_strk_l2_gas_price,
    } = block_context.block_info.gas_prices.strk_gas_prices;

    let minimal_gas_vector =
        estimate_minimal_gas_vector(&block_context, tx, &GasVectorComputationMode::All);

    let default_resource_bounds = AllResourceBounds {
        l1_gas: ResourceBounds {
            max_amount: minimal_gas_vector.l1_gas,
            max_price_per_unit: actual_strk_l1_gas_price.get(),
        },
        l2_gas: ResourceBounds {
            max_amount: minimal_gas_vector.l2_gas,
            max_price_per_unit: actual_strk_l2_gas_price.get(),
        },
        l1_data_gas: ResourceBounds {
            max_amount: minimal_gas_vector.l1_data_gas,
            max_price_per_unit: actual_strk_l1_data_gas_price.get(),
        },
    };

    // Verify successful execution on default resource bounds.
    let valid_resources_tx = invoke_tx_with_default_flags(InvokeTxArgs {
        resource_bounds: ValidResourceBounds::AllResources(default_resource_bounds),
        ..valid_invoke_tx_args.clone()
    })
    .execute(&mut state, &block_context);

    let next_nonce = match valid_resources_tx {
        Ok(_) => 1,
        Err(err) => match err {
            TransactionExecutionError::TransactionPreValidationError(boxed_error) => {
                match *boxed_error {
                    TransactionPreValidationError::TransactionFeeError(boxed_fee_error) => {
                        match *boxed_fee_error {
                            TransactionFeeError::InsufficientResourceBounds { .. } => {
                                panic!("Transaction failed with expected minimal resource bounds.")
                            }
                            // Ignore failures other than those above (e.g., post-validation
                            // errors).
                            _ => 0,
                        }
                    }
                    _ => 0,
                }
            }
            _ => 0,
        },
    };

    // Max gas amount too low, new resource bounds.
    for (insufficient_resource, resource_bounds) in [
        (L1Gas, default_resource_bounds.l1_gas),
        (L2Gas, default_resource_bounds.l2_gas),
        (L1DataGas, default_resource_bounds.l1_data_gas),
    ] {
        if resource_bounds.max_amount == 0_u8.into() {
            continue;
        }
        let mut invalid_resources = default_resource_bounds;
        match insufficient_resource {
            L1Gas => invalid_resources.l1_gas.max_amount.0 -= 1,
            L2Gas => invalid_resources.l2_gas.max_amount.0 -= 1,
            L1DataGas => invalid_resources.l1_data_gas.max_amount.0 -= 1,
        }
        let invalid_v3_tx = invoke_tx_with_default_flags(InvokeTxArgs {
            resource_bounds: ValidResourceBounds::AllResources(invalid_resources),
            nonce: nonce!(next_nonce),
            ..valid_invoke_tx_args.clone()
        });
        let execution_error = invalid_v3_tx.execute(&mut state, &block_context).unwrap_err();
        assert_matches!(
            execution_error,
            TransactionExecutionError::TransactionPreValidationError(boxed_error)
            => assert_matches!(
                *boxed_error,
                TransactionPreValidationError::TransactionFeeError(boxed_fee_error)
                => assert_matches!(
                    *boxed_fee_error,
                    TransactionFeeError::InsufficientResourceBounds{errors}
                    => assert_matches!(
                        errors[0],
                        ResourceBoundsError::MaxGasAmountTooLow{resource, ..}
                        if resource == insufficient_resource
                    )
                )
            )
        );
    }

    // Max gas price too low, new resource bounds.
    for insufficient_resource in [L1Gas, L2Gas, L1DataGas] {
        let mut invalid_resources = default_resource_bounds;
        match insufficient_resource {
            L1Gas => invalid_resources.l1_gas.max_price_per_unit.0 -= 1,
            L2Gas => invalid_resources.l2_gas.max_price_per_unit.0 -= 1,
            L1DataGas => invalid_resources.l1_data_gas.max_price_per_unit.0 -= 1,
        }

        let invalid_v3_tx = invoke_tx_with_default_flags(InvokeTxArgs {
            resource_bounds: ValidResourceBounds::AllResources(invalid_resources),
            nonce: nonce!(next_nonce),
            ..valid_invoke_tx_args.clone()
        });
        let execution_error = invalid_v3_tx.execute(&mut state, &block_context).unwrap_err();
        assert_matches!(
            execution_error,
            TransactionExecutionError::TransactionPreValidationError(boxed_error)
            => assert_matches!(
                *boxed_error,
                TransactionPreValidationError::TransactionFeeError(boxed_fee_error)
                => assert_matches!(
                    *boxed_fee_error,
                    TransactionFeeError::InsufficientResourceBounds{ errors }
                    => assert_matches!(
                        errors[0],
                        ResourceBoundsError::MaxGasPriceTooLow{resource,..}
                        if resource == insufficient_resource
                    )
                )
            )
        );
    }

    // Test several insufficient resources in the same transaction.
    let mut invalid_resources = default_resource_bounds;
    invalid_resources.l2_gas.max_amount.0 -= 1;
    invalid_resources.l1_gas.max_price_per_unit.0 -= 1;
    invalid_resources.l2_gas.max_price_per_unit.0 -= 1;
    invalid_resources.l1_data_gas.max_price_per_unit.0 -= 1;
    let invalid_v3_tx = invoke_tx_with_default_flags(InvokeTxArgs {
        resource_bounds: ValidResourceBounds::AllResources(invalid_resources),
        nonce: nonce!(next_nonce),
        ..valid_invoke_tx_args.clone()
    });
    let execution_error = invalid_v3_tx.execute(&mut state, &block_context).unwrap_err();
    assert_matches!(
        execution_error,
        TransactionExecutionError::TransactionPreValidationError(boxed_error)
        => assert_matches!(
            *boxed_error,
            TransactionPreValidationError::TransactionFeeError(boxed_fee_error)
            => assert_matches!(
                *boxed_fee_error,
                TransactionFeeError::InsufficientResourceBounds{ errors }
                => {
                    assert_eq!(errors.len(), 4);
                    assert_matches!(
                        errors[0],
                        ResourceBoundsError::MaxGasPriceTooLow{resource,..}
                        if resource == L1Gas
                    );
                    assert_matches!(
                        errors[1],
                        ResourceBoundsError::MaxGasPriceTooLow{resource,..}
                        if resource == L1DataGas
                    );
                    assert_matches!(
                        errors[2],
                        ResourceBoundsError::MaxGasAmountTooLow{resource,..}
                        if resource == L2Gas
                    );
                    assert_matches!(
                        errors[3],
                        ResourceBoundsError::MaxGasPriceTooLow{resource,..}
                        if resource == L2Gas
                    );
                }
            )
        )
    );
}

#[rstest]
#[case::with_cairo0_account(CairoVersion::Cairo0)]
#[case::with_cairo1_account(CairoVersion::Cairo1(RunnableCairo1::Casm))]
#[cfg_attr(
    feature = "cairo_native",
    case::with_cairo1_native_account(CairoVersion::Cairo1(RunnableCairo1::Native))
)]
fn test_insufficient_deprecated_resource_bounds_pre_validation(
    block_context: BlockContext,
    #[case] cairo_version: CairoVersion,
) {
    let TestInitData { mut state, account_address, contract_address, .. } =
        create_test_init_data(&block_context.chain_info, cairo_version);
    let valid_invoke_tx_args = invoke_tx_args! {
        sender_address: account_address,
        calldata: create_trivial_calldata(contract_address),
        max_fee: MAX_FEE
    };

    // The minimal gas estimate does not depend on tx version.
    let tx = &invoke_tx_with_default_flags(valid_invoke_tx_args.clone());
    let minimal_l1_gas =
        estimate_minimal_gas_vector(&block_context, tx, &GasVectorComputationMode::NoL2Gas).l1_gas;

    // Test V1 transaction.

    let gas_prices = &block_context.block_info.gas_prices;
    // TODO(Aner): change to linear combination.
    let minimal_fee =
        minimal_l1_gas.checked_mul(gas_prices.eth_gas_prices.l1_gas_price.get()).unwrap();
    // Max fee too low (lower than minimal estimated fee).
    let invalid_max_fee = Fee(minimal_fee.0 - 1);
    let invalid_v1_tx = invoke_tx_with_default_flags(
        invoke_tx_args! { max_fee: invalid_max_fee, version: TransactionVersion::ONE,  ..valid_invoke_tx_args.clone() },
    );
    let execution_error = invalid_v1_tx.execute(&mut state, &block_context).unwrap_err();

    // Test error.
    assert_matches!(
        execution_error,
        TransactionExecutionError::TransactionPreValidationError(boxed_error)
        => assert_matches!(
            *boxed_error,
            TransactionPreValidationError::TransactionFeeError(boxed_fee_error)
            => assert_matches!(
                *boxed_fee_error,
                TransactionFeeError::MaxFeeTooLow {  min_fee, max_fee }
                if max_fee == invalid_max_fee && min_fee == minimal_fee
            )
        )
    );

    // Test V3 transaction.
    let actual_strk_l1_gas_price = gas_prices.strk_gas_prices.l1_gas_price;

    // Max L1 gas amount too low, old resource bounds.
    // TODO(Ori, 1/2/2024): Write an indicative expect message explaining why the conversion works.
    let insufficient_max_l1_gas_amount = (minimal_l1_gas.0 - 1).into();
    let invalid_v3_tx = invoke_tx_with_default_flags(invoke_tx_args! {
        resource_bounds: l1_resource_bounds(insufficient_max_l1_gas_amount, actual_strk_l1_gas_price.into()),
        ..valid_invoke_tx_args.clone()
    });
    let execution_error = invalid_v3_tx.execute(&mut state, &block_context).unwrap_err();
    assert_matches!(
        execution_error,
        TransactionExecutionError::TransactionPreValidationError(boxed_error)
        => assert_matches!(
            *boxed_error,
            TransactionPreValidationError::TransactionFeeError(boxed_fee_error)
            => assert_matches!(
                *boxed_fee_error,
                TransactionFeeError::InsufficientResourceBounds{ errors }
                => assert_matches!(
                    errors[0],
                    ResourceBoundsError::MaxGasAmountTooLow{
                        resource,
                        max_gas_amount,
                        minimal_gas_amount}
                    if max_gas_amount == insufficient_max_l1_gas_amount &&
                    minimal_gas_amount == minimal_l1_gas && resource == L1Gas
                )
            )
        )
    );

    // Max L1 gas price too low, old resource bounds.
    let insufficient_max_l1_gas_price = (actual_strk_l1_gas_price.get().0 - 1).into();
    let invalid_v3_tx = invoke_tx_with_default_flags(invoke_tx_args! {
        resource_bounds: l1_resource_bounds(minimal_l1_gas, insufficient_max_l1_gas_price),
        ..valid_invoke_tx_args.clone()
    });
    let execution_error = invalid_v3_tx.execute(&mut state, &block_context).unwrap_err();
    assert_matches!(
        execution_error,
        TransactionExecutionError::TransactionPreValidationError(boxed_error)
        => assert_matches!(
            *boxed_error,
            TransactionPreValidationError::TransactionFeeError(boxed_fee_error)
            => assert_matches!(
                *boxed_fee_error,
                TransactionFeeError::InsufficientResourceBounds{errors,..}
                => assert_matches!(
                    errors[0],
                    ResourceBoundsError::MaxGasPriceTooLow{ resource: L1Gas ,max_gas_price: max_l1_gas_price, actual_gas_price: actual_l1_gas_price }
                    if max_l1_gas_price == insufficient_max_l1_gas_price &&
                    actual_l1_gas_price == actual_strk_l1_gas_price.into()
                )
            )
        )
    );
}

#[apply(cairo_version)]
#[case::l1_bounds(default_l1_resource_bounds(), Resource::L1Gas)]
#[case::all_bounds_l1_gas_overdraft(default_all_resource_bounds(), Resource::L1Gas)]
#[case::all_bounds_l2_gas_overdraft(default_all_resource_bounds(), Resource::L2Gas)]
#[case::all_bounds_l1_data_gas_overdraft(default_all_resource_bounds(), Resource::L1DataGas)]
fn test_actual_fee_gt_resource_bounds(
    mut block_context: BlockContext,
    #[case] resource_bounds: ValidResourceBounds,
    #[case] overdraft_resource: Resource,
    cairo_version: CairoVersion,
) {
    let account_cairo_version = cairo_version;
    let block_context = &mut block_context;
    block_context.versioned_constants.allocation_cost = AllocationCost::ZERO;
    block_context.block_info.use_kzg_da = true;
    let mut nonce_manager = NonceManager::default();
    let gas_mode = resource_bounds.get_gas_vector_computation_mode();
    let gas_prices = &block_context.block_info.gas_prices.strk_gas_prices;
    let account_contract = FeatureContract::AccountWithoutValidations(account_cairo_version);
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo0);
    let state = &mut test_state(
        &block_context.chain_info,
        BALANCE,
        &[(account_contract, 2), (test_contract, 1)],
    );
    let sender_address0 = account_contract.get_instance_address(0);
    let sender_address1 = account_contract.get_instance_address(1);
    let tx_args = invoke_tx_args! {
        sender_address: sender_address0,
        calldata: create_calldata(
            test_contract.get_instance_address(0), "write_a_lot", &[felt!(2_u8), felt!(7_u8)]
        ),
        resource_bounds,
        nonce: nonce_manager.next(sender_address0),
    };

    // Execute the tx to compute the final gas costs.
    let tx = &invoke_tx_with_default_flags(tx_args.clone());
    let execution_result = tx.execute(state, block_context).unwrap();
    let mut actual_gas = execution_result.receipt.gas;
    let tip = block_context.to_tx_context(tx).effective_tip();

    // Create new gas bounds that are lower than the actual gas.
    let (expected_fee, overdraft_resource_bounds) = match gas_mode {
        GasVectorComputationMode::NoL2Gas => {
            let l1_gas_bound = GasAmount(
                actual_gas.to_l1_gas_for_fee(gas_prices, &block_context.versioned_constants).0 - 1,
            );
            (
                GasVector::from_l1_gas(l1_gas_bound).cost(gas_prices, tip),
                l1_resource_bounds(l1_gas_bound, gas_prices.l1_gas_price.into()),
            )
        }
        GasVectorComputationMode::All => {
            match overdraft_resource {
                Resource::L1Gas => actual_gas.l1_gas.0 -= 1,
                Resource::L2Gas => actual_gas.l2_gas.0 -= 1,
                Resource::L1DataGas => actual_gas.l1_data_gas.0 -= 1,
            }
            (
                actual_gas.cost(gas_prices, tip),
                ValidResourceBounds::all_bounds_from_vectors(&actual_gas, gas_prices),
            )
        }
    };
    let invalid_tx = invoke_tx_with_default_flags(invoke_tx_args! {
        sender_address: sender_address1,
        resource_bounds: overdraft_resource_bounds,
        // To get the same DA cost, write a different value.
        calldata: create_calldata(
            test_contract.get_instance_address(0), "write_a_lot", &[felt!(2_u8), felt!(8_u8)]
        ),
        nonce: nonce_manager.next(sender_address1),
    });
    let execution_result = invalid_tx.execute(state, block_context).unwrap();
    let execution_error = execution_result.revert_error.unwrap();

    // Test error and that fee was charged. Should be at most the fee charged in a successful
    // execution.
    assert!(
        execution_error.to_string().starts_with(&format!("Insufficient max {overdraft_resource}"))
    );
    assert_eq!(execution_result.receipt.fee, expected_fee);
}

#[rstest]
#[case::with_cairo0_account(CairoVersion::Cairo0)]
#[case::with_cairo1_account(CairoVersion::Cairo1(RunnableCairo1::Casm))]
#[cfg_attr(
    feature = "cairo_native",
    case::with_cairo1_native_account(CairoVersion::Cairo1(RunnableCairo1::Native))
)]
fn test_invalid_nonce(
    block_context: BlockContext,
    default_all_resource_bounds: ValidResourceBounds,
    #[case] cairo_version: CairoVersion,
) {
    let TestInitData { mut state, account_address, contract_address, .. } =
        create_test_init_data(&block_context.chain_info, cairo_version);
    let valid_invoke_tx_args = invoke_tx_args! {
        sender_address: account_address,
        calldata: create_trivial_calldata(contract_address),
        resource_bounds: default_all_resource_bounds,
    };
    let mut transactional_state = TransactionalState::create_transactional(&mut state);

    // Strict, negative flow: account nonce = 0, incoming tx nonce = 1.
    let invalid_nonce = nonce!(1_u8);
    let mut invalid_tx = invoke_tx_with_default_flags(
        invoke_tx_args! { nonce: invalid_nonce, ..valid_invoke_tx_args.clone() },
    );
    let invalid_tx_context = block_context.to_tx_context(&invalid_tx);

    invalid_tx.execution_flags.strict_nonce_check = true;
    let pre_validation_err = invalid_tx
        .perform_pre_validation_stage(&mut transactional_state, &invalid_tx_context)
        .unwrap_err();

    // Test error.
    assert_matches!(
        pre_validation_err,
            TransactionPreValidationError::InvalidNonce {address, account_nonce, incoming_tx_nonce}
        if (address, account_nonce, incoming_tx_nonce) ==
        (valid_invoke_tx_args.sender_address, Nonce::default(), invalid_nonce)
    );

    // Non-strict.

    // Positive flow: account nonce = 0, incoming tx nonce = 1.
    let valid_nonce = nonce!(1_u8);
    let mut valid_tx = invoke_tx_with_default_flags(
        invoke_tx_args! { nonce: valid_nonce, ..valid_invoke_tx_args.clone() },
    );

    let valid_tx_context = block_context.to_tx_context(&valid_tx);

    valid_tx.execution_flags.strict_nonce_check = false;
    valid_tx.perform_pre_validation_stage(&mut transactional_state, &valid_tx_context).unwrap();

    // Negative flow: account nonce = 1, incoming tx nonce = 0.
    let invalid_nonce = nonce!(0_u8);
    let mut invalid_tx = invoke_tx_with_default_flags(
        invoke_tx_args! { nonce: invalid_nonce, ..valid_invoke_tx_args.clone() },
    );
    let invalid_tx_context = block_context.to_tx_context(&invalid_tx);

    invalid_tx.execution_flags.strict_nonce_check = false;
    let pre_validation_err = invalid_tx
        .perform_pre_validation_stage(&mut transactional_state, &invalid_tx_context)
        .unwrap_err();

    // Test error.
    assert_matches!(
        pre_validation_err,
        TransactionPreValidationError::InvalidNonce {address, account_nonce, incoming_tx_nonce}
        if (address, account_nonce, incoming_tx_nonce) ==
        (valid_invoke_tx_args.sender_address, nonce!(1_u8), invalid_nonce)
    );
}

/// Expected CallInfo for `__validate__` call in a declare transaction.
fn declare_validate_callinfo(
    version: TransactionVersion,
    declared_contract_version: CairoVersion,
    declared_class_hash: ClassHash,
    account_class_hash: ClassHash,
    account_address: ContractAddress,
    tracked_resource: TrackedResource,
    user_initial_gas: Option<GasAmount>,
) -> Option<CallInfo> {
    // V0 transactions do not run validate.
    if version == TransactionVersion::ZERO {
        None
    } else {
        let gas_consumed = match declared_contract_version {
            CairoVersion::Cairo0 => 0,
            CairoVersion::Cairo1(_) => {
                VersionedConstants::create_for_testing()
                    .os_constants
                    .gas_costs
                    .base
                    .entry_point_initial_budget
                    - *DECLARE_REDEPOSIT_AMOUNT
            }
        };
        expected_validate_call_info(
            account_class_hash,
            constants::VALIDATE_DECLARE_ENTRY_POINT_NAME,
            gas_consumed,
            calldata![declared_class_hash.0],
            account_address,
            declared_contract_version,
            tracked_resource,
            user_initial_gas,
        )
    }
}

/// Returns the expected used L1 gas and blob gas (according to use_kzg_da flag) due to execution of
/// a declare transaction.
fn declare_expected_state_changes_count(version: TransactionVersion) -> StateChangesCount {
    // TODO(Dori): Make TransactionVersion an enum and use match here.
    if version == TransactionVersion::ZERO {
        StateChangesCount {
            n_storage_updates: 1, // Sender balance.
            ..StateChangesCount::default()
        }
    } else if version == TransactionVersion::ONE {
        StateChangesCount {
            n_storage_updates: 1,    // Sender balance.
            n_modified_contracts: 1, // Nonce.
            ..StateChangesCount::default()
        }
    } else if version == TransactionVersion::TWO || version == TransactionVersion::THREE {
        StateChangesCount {
            n_storage_updates: 1,             // Sender balance.
            n_modified_contracts: 1,          // Nonce.
            n_compiled_class_hash_updates: 1, // Also set compiled class hash.
            ..StateChangesCount::default()
        }
    } else {
        panic!("Unsupported version {version:?}.")
    }
}

#[rstest]
fn test_declare_redeposit_amount_regression() {
    expect![[r#"
        7160
    "#]]
    .assert_debug_eq(&*DECLARE_REDEPOSIT_AMOUNT);
}

#[apply(cairo_version)]
#[case(TransactionVersion::ZERO, CairoVersion::Cairo0)]
#[case(TransactionVersion::ONE, CairoVersion::Cairo0)]
#[case(TransactionVersion::TWO, CairoVersion::Cairo1(RunnableCairo1::Casm))]
#[case(TransactionVersion::THREE, CairoVersion::Cairo1(RunnableCairo1::Casm))]
fn test_declare_tx(
    default_all_resource_bounds: ValidResourceBounds,
    cairo_version: CairoVersion,
    #[case] tx_version: TransactionVersion,
    #[case] empty_contract_version: CairoVersion,
    #[values(false, true)] use_kzg_da: bool,
) {
    let account_cairo_version = cairo_version;
    let block_context = &BlockContext::create_for_account_testing_with_kzg(use_kzg_da);
    let versioned_constants = &block_context.versioned_constants;
    let empty_contract = FeatureContract::Empty(empty_contract_version);
    let account = FeatureContract::AccountWithoutValidations(account_cairo_version);
    let chain_info = &block_context.chain_info;
    let state = &mut test_state(chain_info, BALANCE, &[(account, 1)]);
    let class_hash = empty_contract.get_class_hash();
    let compiled_class_hash = empty_contract.get_compiled_class_hash();
    let class_info = calculate_class_info_for_testing(empty_contract.get_class());
    let sender_address = account.get_instance_address(0);
    let mut nonce_manager = NonceManager::default();
    let state_changes_for_fee = declare_expected_state_changes_count(tx_version);
    let starknet_resources = StarknetResources::new(
        0,
        0,
        class_info.code_size(),
        StateResources::new_for_testing(state_changes_for_fee, 0),
        None,
        ExecutionSummary::default(),
    );
    let account_tx = AccountTransaction::new_with_default_flags(executable_declare_tx(
        declare_tx_args! {
            max_fee: MAX_FEE,
            sender_address,
            version: tx_version,
            resource_bounds: default_all_resource_bounds,
            class_hash,
            compiled_class_hash,
            nonce: nonce_manager.next(sender_address),
        },
        class_info.clone(),
    ));

    // Check state before transaction application.
    assert_matches!(
        state.get_compiled_class(class_hash).unwrap_err(),
        StateError::UndeclaredClassHash(undeclared_class_hash) if
        undeclared_class_hash == class_hash
    );
    let fee_type = &account_tx.fee_type();
    let tx_context = &block_context.to_tx_context(&account_tx);
    let actual_execution_info = account_tx.execute(state, block_context).unwrap();
    assert_eq!(actual_execution_info.revert_error, None);

    // Build expected validate call info.
    let expected_validate_call_info = declare_validate_callinfo(
        tx_version,
        account_cairo_version,
        class_hash,
        account.get_class_hash(),
        sender_address,
        account
            .get_runnable_class()
            .tracked_resource(&versioned_constants.min_sierra_version_for_sierra_gas, None),
        if tx_version >= TransactionVersion::THREE {
            Some(user_initial_gas_from_bounds(default_all_resource_bounds, Some(block_context)))
        } else {
            None
        },
    );

    // Build expected fee transfer call info.
    let expected_actual_fee = actual_execution_info.receipt.fee;
    // V0 transactions do not handle fee.
    let expected_fee_transfer_call_info = if tx_version == TransactionVersion::ZERO {
        None
    } else {
        expected_fee_transfer_call_info(
            tx_context,
            sender_address,
            expected_actual_fee,
            FeatureContract::ERC20(cairo_version).get_class_hash(),
            cairo_version,
        )
    };

    let da_gas = starknet_resources.state.da_gas_vector(use_kzg_da);
    let (expected_tx_cairo_resources, expected_os_cairo_resources) = get_expected_cairo_resources(
        versioned_constants,
        TransactionType::Declare,
        &starknet_resources,
        vec![&expected_validate_call_info],
    );
    let initial_gas = VersionedConstants::create_for_testing()
        .os_constants
        .gas_costs
        .base
        .entry_point_initial_budget;
    let expected_gas_consumed = match account_cairo_version {
        CairoVersion::Cairo0 => GasAmount(0),
        CairoVersion::Cairo1(_) => {
            // V0 transactions do not handle fee.
            if tx_version == TransactionVersion::ZERO {
                GasAmount(0)
            } else {
                GasAmount(initial_gas - *DECLARE_REDEPOSIT_AMOUNT)
            }
        }
    };

    let mut expected_actual_resources = TransactionResources {
        starknet_resources,
        computation: ComputationResources {
            tx_vm_resources: expected_tx_cairo_resources,
            os_vm_resources: expected_os_cairo_resources,
            sierra_gas: expected_gas_consumed,
            ..Default::default()
        },
    };

    add_kzg_da_resources_to_resources_mapping(
        &mut expected_actual_resources.computation.os_vm_resources,
        &state_changes_for_fee,
        versioned_constants,
        use_kzg_da,
    );

    let expected_total_gas = expected_actual_resources.to_gas_vector(
        versioned_constants,
        use_kzg_da,
        &tx_context.get_gas_vector_computation_mode(),
    );

    let expected_execution_info = TransactionExecutionInfo {
        validate_call_info: expected_validate_call_info,
        execute_call_info: None,
        fee_transfer_call_info: expected_fee_transfer_call_info,
        receipt: TransactionReceipt {
            fee: expected_actual_fee,
            da_gas,
            resources: expected_actual_resources,
            gas: expected_total_gas,
        },
        revert_error: None,
    };

    // Test execution info result.
    assert_eq!(actual_execution_info, expected_execution_info);

    // Test nonce update. V0 transactions do not update nonce.
    let expected_nonce = nonce!(if tx_version == TransactionVersion::ZERO { 0_u8 } else { 1_u8 });
    let nonce_from_state = state.get_nonce_at(sender_address).unwrap();
    assert_eq!(nonce_from_state, expected_nonce);

    // Test final balances.
    validate_final_balances(
        state,
        chain_info,
        expected_actual_fee,
        get_fee_token_var_address(sender_address),
        fee_type,
        BALANCE,
        BALANCE,
    );

    // Verify class declaration.
    let contract_class_from_state = state.get_compiled_class(class_hash).unwrap();
    assert_eq!(contract_class_from_state, class_info.contract_class().try_into().unwrap());

    // Checks that redeclaring the same contract fails.
    let account_tx2 = AccountTransaction::new_with_default_flags(executable_declare_tx(
        declare_tx_args! {
            max_fee: MAX_FEE,
            sender_address,
            version: tx_version,
            resource_bounds: default_all_resource_bounds,
            class_hash,
            compiled_class_hash,
            nonce: nonce_manager.next(sender_address),
        },
        class_info.clone(),
    ));
    let result = account_tx2.execute(state, block_context);
    assert_matches!(
         result.unwrap_err(),
        TransactionExecutionError::DeclareTransactionError{ class_hash:already_declared_class_hash } if
        already_declared_class_hash == class_hash
    );
}

#[rstest]
fn test_declare_tx_v0(
    block_context: BlockContext,
    default_l1_resource_bounds: ValidResourceBounds,
) {
    let TestInitData { mut state, account_address, mut nonce_manager, .. } = create_test_init_data(
        &block_context.chain_info,
        CairoVersion::Cairo1(RunnableCairo1::Casm),
    );
    let empty_contract = FeatureContract::Empty(CairoVersion::Cairo0);
    let class_hash = empty_contract.get_class_hash();
    let compiled_class_hash = empty_contract.get_compiled_class_hash();
    let class_info = calculate_class_info_for_testing(empty_contract.get_class());

    let tx = executable_declare_tx(
        declare_tx_args! {
            max_fee: Fee(0),
            sender_address: account_address,
            version: TransactionVersion::ZERO,
            resource_bounds: default_l1_resource_bounds,
            class_hash,
            compiled_class_hash,
            nonce: nonce_manager.next(account_address),
        },
        class_info.clone(),
    );
    let account_tx = AccountTransaction {
        tx,
        execution_flags: ExecutionFlags { charge_fee: false, ..ExecutionFlags::default() },
    };

    // fee not charged for declare v0.
    let actual_execution_info = account_tx.execute(&mut state, &block_context).unwrap();

    assert_eq!(actual_execution_info.fee_transfer_call_info, None, "not none");
    assert_eq!(actual_execution_info.receipt.fee, Fee(0));
}

#[rstest]
fn test_deploy_account_redeposit_amount_regression() {
    expect![[r#"
        6760
    "#]]
    .assert_debug_eq(&*DEPLOY_ACCOUNT_REDEPOSIT_AMOUNT);
}

#[rstest]
#[case::with_cairo0_account(CairoVersion::Cairo0, 0)]
#[case::with_cairo1_account(
    CairoVersion::Cairo1(RunnableCairo1::Casm),
    VersionedConstants::create_for_testing().os_constants.gas_costs.base.entry_point_initial_budget - *DEPLOY_ACCOUNT_REDEPOSIT_AMOUNT
)]
#[cfg_attr(
    feature = "cairo_native",
    case::with_cairo1_native_account(
        CairoVersion::Cairo1(RunnableCairo1::Native),
        VersionedConstants::create_for_testing().os_constants.gas_costs.base.entry_point_initial_budget - *DEPLOY_ACCOUNT_REDEPOSIT_AMOUNT
    )
)]
fn test_deploy_account_tx(
    #[case] cairo_version: CairoVersion,
    #[values(false, true)] use_kzg_da: bool,
    #[case] expected_gas_consumed: u64,
    default_all_resource_bounds: ValidResourceBounds,
) {
    let block_context = &BlockContext::create_for_account_testing_with_kzg(use_kzg_da);
    let versioned_constants = &block_context.versioned_constants;
    let chain_info = &block_context.chain_info;
    let mut nonce_manager = NonceManager::default();
    let account = FeatureContract::AccountWithoutValidations(cairo_version);
    let account_class_hash = account.get_class_hash();
    let state = &mut test_state(chain_info, BALANCE, &[(account, 1)]);
    let deploy_account = AccountTransaction::new_with_default_flags(
        create_executable_deploy_account_tx_and_update_nonce(
            deploy_account_tx_args! {
                resource_bounds: default_all_resource_bounds,
                class_hash: account_class_hash
            },
            &mut nonce_manager,
        ),
    );

    // Extract deploy account transaction fields for testing, as it is consumed when creating an
    // account transaction.
    let class_hash = deploy_account.class_hash().unwrap();
    let deployed_account_address = deploy_account.sender_address();
    let user_initial_gas =
        user_initial_gas_from_bounds(default_all_resource_bounds, Some(block_context));

    // Update the balance of the about to be deployed account contract in the erc20 contract, so it
    // can pay for the transaction execution.
    let deployed_account_balance_key = get_fee_token_var_address(deployed_account_address);

    fund_account(chain_info, deploy_account.tx.contract_address(), BALANCE, &mut state.state);

    let fee_type = &deploy_account.fee_type();
    let tx_context = &block_context.to_tx_context(&deploy_account);
    let actual_execution_info = deploy_account.execute(state, block_context).unwrap();

    // Build expected validate call info.
    let validate_calldata = if let ApiExecutableTransaction::DeployAccount(tx) = &deploy_account.tx
    {
        Calldata(
            [
                vec![class_hash.0, tx.contract_address_salt().0],
                (*tx.constructor_calldata().0).clone(),
            ]
            .concat()
            .into(),
        )
    } else {
        panic!("Expected DeployAccount transaction.")
    };

    let tracked_resource = account
        .get_runnable_class()
        .tracked_resource(&versioned_constants.min_sierra_version_for_sierra_gas, None);
    let expected_validate_call_info = expected_validate_call_info(
        account_class_hash,
        constants::VALIDATE_DEPLOY_ENTRY_POINT_NAME,
        expected_gas_consumed,
        validate_calldata,
        deployed_account_address,
        cairo_version,
        tracked_resource,
        Some(user_initial_gas),
    );

    // Build expected execute call info.
    let expected_execute_initial_gas = match tracked_resource {
        TrackedResource::CairoSteps => versioned_constants.infinite_gas_for_vm_mode(),
        TrackedResource::SierraGas => {
            user_initial_gas
        // Note that in the case of deploy account, the initial gas in "execute" is limited by
        // max_validation_sierra_gas.
        .min(versioned_constants.os_constants.validate_max_sierra_gas).0
        }
    };
    let expected_execute_call_info = Some(CallInfo {
        call: CallEntryPoint {
            class_hash: Some(account_class_hash),
            code_address: None,
            entry_point_type: EntryPointType::Constructor,
            entry_point_selector: selector_from_name(CONSTRUCTOR_ENTRY_POINT_NAME),
            storage_address: deployed_account_address,
            initial_gas: expected_execute_initial_gas,
            ..Default::default()
        },
        tracked_resource,
        ..Default::default()
    });

    // Build expected fee transfer call info.
    let expected_actual_fee = actual_execution_info.receipt.fee;
    let expected_fee_transfer_call_info = expected_fee_transfer_call_info(
        tx_context,
        deployed_account_address,
        expected_actual_fee,
        FeatureContract::ERC20(cairo_version).get_class_hash(),
        cairo_version,
    );
    let starknet_resources = actual_execution_info.receipt.resources.starknet_resources.clone();

    let state_changes_count = StateChangesCount {
        n_storage_updates: 1,
        n_modified_contracts: 1,
        n_class_hash_updates: 1,
        ..StateChangesCount::default()
    };
    let da_gas = get_da_gas_cost(&state_changes_count, use_kzg_da);
    let (expected_tx_cairo_resources, expected_os_cairo_resources) = get_expected_cairo_resources(
        &block_context.versioned_constants,
        TransactionType::DeployAccount,
        &starknet_resources,
        vec![&expected_validate_call_info, &expected_execute_call_info],
    );

    let mut actual_resources = TransactionResources {
        starknet_resources,
        computation: ComputationResources {
            tx_vm_resources: expected_tx_cairo_resources,
            os_vm_resources: expected_os_cairo_resources,
            sierra_gas: expected_gas_consumed.into(),
            ..Default::default()
        },
    };

    add_kzg_da_resources_to_resources_mapping(
        &mut actual_resources.computation.os_vm_resources,
        &state_changes_count,
        versioned_constants,
        use_kzg_da,
    );

    let expected_total_gas = actual_resources.to_gas_vector(
        &block_context.versioned_constants,
        block_context.block_info.use_kzg_da,
        &tx_context.get_gas_vector_computation_mode(),
    );

    let expected_execution_info = TransactionExecutionInfo {
        validate_call_info: expected_validate_call_info,
        execute_call_info: expected_execute_call_info,
        fee_transfer_call_info: expected_fee_transfer_call_info,
        receipt: TransactionReceipt {
            fee: expected_actual_fee,
            da_gas,
            resources: actual_resources,
            gas: expected_total_gas,
        },
        revert_error: None,
    };

    // Test execution info result.
    assert_eq!(actual_execution_info, expected_execution_info);

    // Test nonce update.
    let nonce_from_state = state.get_nonce_at(deployed_account_address).unwrap();
    assert_eq!(nonce_from_state, nonce!(1_u8));

    // Test final balances.
    validate_final_balances(
        state,
        chain_info,
        expected_actual_fee,
        deployed_account_balance_key,
        fee_type,
        BALANCE,
        BALANCE,
    );

    // Verify deployment.
    let class_hash_from_state = state.get_class_hash_at(deployed_account_address).unwrap();
    assert_eq!(class_hash_from_state, class_hash);

    // Negative flow.
    // Deploy to an existing address.
    let mut tx: ApiExecutableTransaction = executable_deploy_account_tx(deploy_account_tx_args! {
        resource_bounds: default_all_resource_bounds,
        class_hash: account_class_hash
    });
    let nonce = nonce_manager.next(tx.contract_address());
    if let ApiExecutableTransaction::DeployAccount(DeployAccountTransaction {
        ref mut tx, ..
    }) = tx
    {
        match tx {
            starknet_api::transaction::DeployAccountTransaction::V1(ref mut tx) => tx.nonce = nonce,
            starknet_api::transaction::DeployAccountTransaction::V3(ref mut tx) => tx.nonce = nonce,
        }
    }
    let deploy_account = AccountTransaction::new_with_default_flags(tx);
    let error = deploy_account.execute(state, block_context).unwrap_err();
    assert_matches!(
        error,
        TransactionExecutionError::ContractConstructorExecutionFailed(
            ConstructorEntryPointExecutionError::ExecutionError { error, .. }
        )
        if matches!(*error, EntryPointExecutionError::StateError(
            StateError::UnavailableContractAddress(_)
        ))
    );
}

#[rstest]
fn test_fail_deploy_account_undeclared_class_hash(
    block_context: BlockContext,
    default_all_resource_bounds: ValidResourceBounds,
) {
    let block_context = &block_context;
    let chain_info = &block_context.chain_info;
    let state = &mut test_state(chain_info, BALANCE, &[]);
    let undeclared_hash = class_hash!("0xdeadbeef");
    let deploy_account = AccountTransaction::new_with_default_flags(executable_deploy_account_tx(
        deploy_account_tx_args! {
            resource_bounds: default_all_resource_bounds,  class_hash: undeclared_hash
        },
    ));
    let tx_context = block_context.to_tx_context(&deploy_account);
    let fee_type = tx_context.tx_info.fee_type();

    // Fund account, so as not to fail pre-validation.
    state
        .set_storage_at(
            chain_info.fee_token_address(&fee_type),
            get_fee_token_var_address(deploy_account.sender_address()),
            felt!(BALANCE.0),
        )
        .unwrap();

    let error = deploy_account.execute(state, block_context).unwrap_err();
    assert_matches!(
        error,
        TransactionExecutionError::ContractConstructorExecutionFailed(
            ConstructorEntryPointExecutionError::ExecutionError { error, .. }
        )
        if matches!(
            *error,
            EntryPointExecutionError::StateError(StateError::UndeclaredClassHash(class_hash))
            if class_hash == undeclared_hash
        )
    );
}

#[cfg(feature = "cairo_native")]
fn check_native_validate_error(
    error: TransactionExecutionError,
    error_msg: &str,
    validate_constructor: bool,
) {
    let syscall_error = match error {
        TransactionExecutionError::ValidateTransactionError { error: boxed_error, .. } => {
            match *boxed_error {
                EntryPointExecutionError::NativeUnrecoverableError(boxed_syscall_error) => {
                    assert!(!validate_constructor);
                    boxed_syscall_error
                }
                _ => panic!("Unexpected error: {boxed_error:?}"),
            }
        }
        TransactionExecutionError::ContractConstructorExecutionFailed(
            ConstructorEntryPointExecutionError::ExecutionError { error: boxed_error, .. },
        ) => match *boxed_error {
            EntryPointExecutionError::NativeUnrecoverableError(boxed_syscall_error) => {
                assert!(validate_constructor);
                boxed_syscall_error
            }
            _ => panic!("Unexpected error: {boxed_error:?}"),
        },
        _ => panic!("Unexpected error: {:?}", &error),
    };
    assert_matches!(
        *syscall_error,
        SyscallExecutionError::SyscallExecutorBase(
            SyscallExecutorBaseError::InvalidSyscallInExecutionMode { .. }
        )
    );
    assert!(syscall_error.to_string().contains(error_msg));
}
// TODO(Arni, 1/5/2024): Cover other versions of declare transaction.
// TODO(Arni, 1/5/2024): Consider version 0 invoke.
#[apply(cairo_version)]
#[case::validate_version_1(TransactionType::InvokeFunction, false, TransactionVersion::ONE)]
#[case::validate_version_3(TransactionType::InvokeFunction, false, TransactionVersion::THREE)]
#[case::validate_declare_version_1(TransactionType::Declare, false, TransactionVersion::ONE)]
#[case::validate_declare_version_2(TransactionType::Declare, false, TransactionVersion::TWO)]
#[case::validate_declare_version_3(TransactionType::Declare, false, TransactionVersion::THREE)]
#[case::validate_deploy_version_1(TransactionType::DeployAccount, false, TransactionVersion::ONE)]
#[case::validate_deploy_version_3(TransactionType::DeployAccount, false, TransactionVersion::THREE)]
#[case::constructor_version_1(TransactionType::DeployAccount, true, TransactionVersion::ONE)]
#[case::constructor_version_3(TransactionType::DeployAccount, true, TransactionVersion::THREE)]
fn test_validate_accounts_tx(
    block_context: BlockContext,
    #[case] tx_type: TransactionType,
    #[case] validate_constructor: bool,
    #[case] tx_version: TransactionVersion,
    cairo_version: CairoVersion,
) {
    let block_context = &block_context;
    let account_balance = Fee(0);
    let faulty_account = FeatureContract::FaultyAccount(cairo_version);
    let sender_address = faulty_account.get_instance_address(0);
    let class_hash = faulty_account.get_class_hash();
    let state = &mut test_state(&block_context.chain_info, account_balance, &[(faulty_account, 1)]);
    let salt_manager = &mut SaltManager::default();

    let default_args = FaultyAccountTxCreatorArgs {
        tx_type,
        tx_version,
        sender_address,
        class_hash,
        validate_constructor,
        validate: true,
        charge_fee: false, // We test `__validate__`, and don't care about the charge fee flow.
        ..Default::default()
    };

    // Negative flows.

    // Logic failure.
    let account_tx = create_account_tx_for_validate_test_nonce_0(FaultyAccountTxCreatorArgs {
        scenario: INVALID,
        contract_address_salt: salt_manager.next_salt(),
        additional_data: None,
        ..default_args
    });

    let error = account_tx.execute(state, block_context).unwrap_err();
    check_tx_execution_error_for_invalid_scenario!(cairo_version, error, validate_constructor,);

    // Try to call another contract (forbidden).
    let account_tx = create_account_tx_for_validate_test_nonce_0(FaultyAccountTxCreatorArgs {
        scenario: CALL_CONTRACT,
        additional_data: Some(vec![felt!("0x1991")]), /* Some address different than
                                                       * the address of
                                                       * faulty_account. */
        contract_address_salt: salt_manager.next_salt(),
        resource_bounds: ValidResourceBounds::create_for_testing_no_fee_enforcement(),
        ..default_args
    });
    let error = account_tx.execute(state, block_context).unwrap_err();
    match cairo_version {
        CairoVersion::Cairo0 | CairoVersion::Cairo1(RunnableCairo1::Casm) => {
            check_tx_execution_error_for_custom_hint!(
                error,
                "Unauthorized syscall call_contract in execution mode Validate.",
                validate_constructor,
            );
        }
        #[cfg(feature = "cairo_native")]
        CairoVersion::Cairo1(RunnableCairo1::Native) => {
            check_native_validate_error(
                error,
                "Unauthorized syscall call_contract in execution mode Validate.",
                validate_constructor,
            );
        }
    }

    if let CairoVersion::Cairo1(runnable_cairo1) = cairo_version {
        // Try to use the syscall get_block_hash (forbidden).
        let account_tx = create_account_tx_for_validate_test_nonce_0(FaultyAccountTxCreatorArgs {
            scenario: GET_BLOCK_HASH,
            contract_address_salt: salt_manager.next_salt(),
            additional_data: None,
            resource_bounds: ValidResourceBounds::create_for_testing_no_fee_enforcement(),
            ..default_args
        });
        let error = account_tx.execute(state, block_context).unwrap_err();
        match runnable_cairo1 {
            RunnableCairo1::Casm => {
                check_tx_execution_error_for_custom_hint!(
                    &error,
                    "Unauthorized syscall get_block_hash on recent blocks in execution mode \
                     Validate.",
                    validate_constructor,
                );
            }
            #[cfg(feature = "cairo_native")]
            RunnableCairo1::Native => {
                check_native_validate_error(
                    error,
                    "Unauthorized syscall get_block_hash on recent blocks in execution mode \
                     Validate.",
                    validate_constructor,
                );
            }
        }
    }
    if let CairoVersion::Cairo0 = cairo_version {
        // Try to use the syscall get_sequencer_address (forbidden).
        let account_tx = create_account_tx_for_validate_test_nonce_0(FaultyAccountTxCreatorArgs {
            scenario: GET_SEQUENCER_ADDRESS,
            contract_address_salt: salt_manager.next_salt(),
            resource_bounds: ValidResourceBounds::create_for_testing_no_fee_enforcement(),
            ..default_args
        });
        let error = account_tx.execute(state, block_context).unwrap_err();
        check_tx_execution_error_for_custom_hint!(
            &error,
            "Unauthorized syscall get_sequencer_address in execution mode Validate.",
            validate_constructor,
        );
    }

    // Positive flows.

    // Valid logic.
    let nonce_manager = &mut NonceManager::default();
    let declared_contract_cairo_version = CairoVersion::from_declare_tx_version(tx_version);
    let account_tx = create_account_tx_for_validate_test(
        nonce_manager,
        FaultyAccountTxCreatorArgs {
            scenario: VALID,
            contract_address_salt: salt_manager.next_salt(),
            additional_data: None,
            declared_contract: Some(FeatureContract::TestContract(declared_contract_cairo_version)),
            resource_bounds: ValidResourceBounds::create_for_testing_no_fee_enforcement(),
            ..default_args
        },
    );
    let result = account_tx.execute(state, block_context);
    assert!(result.is_ok(), "Execution failed: {:?}", result.unwrap_err());

    if tx_type != TransactionType::DeployAccount {
        // Call self (allowed).
        let account_tx = create_account_tx_for_validate_test(
            nonce_manager,
            FaultyAccountTxCreatorArgs {
                scenario: CALL_CONTRACT,
                additional_data: Some(vec![*sender_address.0.key()]),
                declared_contract: Some(FeatureContract::AccountWithLongValidate(
                    declared_contract_cairo_version,
                )),
                resource_bounds: ValidResourceBounds::create_for_testing_no_fee_enforcement(),
                ..default_args
            },
        );
        let result = account_tx.execute(state, block_context);
        assert!(result.is_ok(), "Execution failed: {:?}", result.unwrap_err());
    }

    if let CairoVersion::Cairo0 = cairo_version {
        // Call the syscall get_block_number and assert the returned block number was modified
        // for validate.
        let account_tx = create_account_tx_for_validate_test(
            nonce_manager,
            FaultyAccountTxCreatorArgs {
                scenario: GET_BLOCK_NUMBER,
                contract_address_salt: salt_manager.next_salt(),
                additional_data: Some(vec![Felt::from(CURRENT_BLOCK_NUMBER_FOR_VALIDATE)]),
                declared_contract: Some(FeatureContract::AccountWithoutValidations(
                    declared_contract_cairo_version,
                )),
                resource_bounds: ValidResourceBounds::create_for_testing_no_fee_enforcement(),
                ..default_args
            },
        );
        let result = account_tx.execute(state, block_context);
        assert!(result.is_ok(), "Execution failed: {:?}", result.unwrap_err());

        // Call the syscall get_block_timestamp and assert the returned timestamp was modified
        // for validate.
        let account_tx = create_account_tx_for_validate_test(
            nonce_manager,
            FaultyAccountTxCreatorArgs {
                scenario: GET_BLOCK_TIMESTAMP,
                contract_address_salt: salt_manager.next_salt(),
                additional_data: Some(vec![Felt::from(CURRENT_BLOCK_TIMESTAMP_FOR_VALIDATE)]),
                declared_contract: Some(FeatureContract::Empty(declared_contract_cairo_version)),
                resource_bounds: ValidResourceBounds::create_for_testing_no_fee_enforcement(),
                ..default_args
            },
        );
        let result = account_tx.execute(state, block_context);
        assert!(result.is_ok(), "Execution failed: {:?}", result.unwrap_err());
    }

    if let CairoVersion::Cairo1(RunnableCairo1::Casm) = cairo_version {
        let account_tx = create_account_tx_for_validate_test(
            // Call the syscall get_execution_info and assert the returned block_info was
            // modified for validate.
            nonce_manager,
            FaultyAccountTxCreatorArgs {
                scenario: GET_EXECUTION_INFO,
                contract_address_salt: salt_manager.next_salt(),
                additional_data: Some(vec![
                    Felt::from(CURRENT_BLOCK_NUMBER_FOR_VALIDATE),
                    Felt::from(CURRENT_BLOCK_TIMESTAMP_FOR_VALIDATE),
                    Felt::ZERO, // Sequencer address for validate.
                ]),
                declared_contract: Some(FeatureContract::Empty(declared_contract_cairo_version)),
                resource_bounds: ValidResourceBounds::create_for_testing_no_fee_enforcement(),
                ..default_args
            },
        );
        let result = account_tx.execute(state, block_context);
        assert!(result.is_ok(), "Execution failed: {:?}", result.unwrap_err());
    }
}

#[apply(two_cairo_versions)]
fn test_valid_flag(
    block_context: BlockContext,
    default_all_resource_bounds: ValidResourceBounds,
    cairo_version1: CairoVersion,
    cairo_version2: CairoVersion,
) {
    let account_cairo_version = cairo_version1;
    let test_contract_cairo_version = cairo_version2;
    let block_context = &block_context;
    let account_contract = FeatureContract::AccountWithoutValidations(account_cairo_version);
    let test_contract = FeatureContract::TestContract(test_contract_cairo_version);
    let state = &mut test_state(
        &block_context.chain_info,
        BALANCE,
        &[(account_contract, 1), (test_contract, 1)],
    );

    let tx = executable_invoke_tx(invoke_tx_args! {
        sender_address: account_contract.get_instance_address(0),
        calldata: create_trivial_calldata(test_contract.get_instance_address(0)),
        resource_bounds: default_all_resource_bounds,
    });
    let account_tx = AccountTransaction {
        tx,
        execution_flags: ExecutionFlags { validate: false, ..ExecutionFlags::default() },
    };

    let actual_execution_info = account_tx.execute(state, block_context).unwrap();

    assert!(actual_execution_info.validate_call_info.is_none());
}

// TODO(Noa,01/12/2023): Consider moving it to syscall_test.
#[rstest]
fn test_only_query_flag(
    block_context: BlockContext,
    default_all_resource_bounds: ValidResourceBounds,
    #[values(true, false)] only_query: bool,
) {
    let TestInitData { mut state, account_address, contract_address, .. } = create_test_init_data(
        &block_context.chain_info,
        CairoVersion::Cairo1(RunnableCairo1::Casm),
    );
    let mut version = Felt::from(3_u8);
    if only_query {
        version += *QUERY_VERSION_BASE;
    }
    let expected_tx_info = vec![
        version,                              // Transaction version.
        *account_address.0.key(),             // Account address.
        Felt::ZERO,                           // Max fee.
        Felt::ZERO,                           // Signature.
        Felt::ZERO,                           // Transaction hash.
        felt!(&*CHAIN_ID_FOR_TESTS.as_hex()), // Chain ID.
        Felt::ZERO,                           // Nonce.
    ];

    let expected_resource_bounds = vec![
        Felt::THREE,                                   // Length of ResourceBounds array.
        felt!(L1Gas.to_hex()),                         // Resource.
        felt!(DEFAULT_L1_GAS_AMOUNT.0),                // Max amount.
        felt!(DEFAULT_STRK_L1_GAS_PRICE.get().0),      // Max price per unit.
        felt!(L2Gas.to_hex()),                         // Resource.
        felt!(DEFAULT_L2_GAS_MAX_AMOUNT.0),            // Max amount.
        felt!(DEFAULT_STRK_L2_GAS_PRICE.get().0),      // Max price per unit.
        felt!(L1DataGas.to_hex()),                     // Resource.
        felt!(DEFAULT_L1_DATA_GAS_MAX_AMOUNT.0),       // Max amount.
        felt!(DEFAULT_STRK_L1_DATA_GAS_PRICE.get().0), // Max price per unit.
    ];

    let expected_unsupported_fields = vec![
        Felt::ZERO, // Tip.
        Felt::ZERO, // Paymaster data.
        Felt::ZERO, // Nonce DA.
        Felt::ZERO, // Fee DA.
        Felt::ZERO, // Account data.
    ];

    let entry_point_selector = selector_from_name("test_get_execution_info");
    let expected_call_info = vec![
        *account_address.0.key(),  // Caller address.
        *contract_address.0.key(), // Storage address.
        entry_point_selector.0,    // Entry point selector.
    ];
    let expected_block_info = [
        felt!(CURRENT_BLOCK_NUMBER),    // Block number.
        felt!(CURRENT_BLOCK_TIMESTAMP), // Block timestamp.
        felt!(TEST_SEQUENCER_ADDRESS),  // Sequencer address.
    ];
    let calldata_len = expected_block_info.len()
        + expected_tx_info.len()
        + expected_resource_bounds.len()
        + expected_unsupported_fields.len()
        + expected_call_info.len();
    let execute_calldata = vec![
        *contract_address.0.key(), // Contract address.
        entry_point_selector.0,    // EP selector.
        // TODO(Ori, 1/2/2024): Write an indicative expect message explaining why the conversion
        // works.
        felt!(u64::try_from(calldata_len).expect("Failed to convert usize to u64.")), /* Calldata length. */
    ];
    let execute_calldata = Calldata(
        [
            execute_calldata,
            expected_block_info.clone().to_vec(),
            expected_tx_info,
            expected_resource_bounds,
            expected_unsupported_fields,
            expected_call_info,
        ]
        .concat()
        .into(),
    );
    let tx = executable_invoke_tx(invoke_tx_args! {
        calldata: execute_calldata,
        resource_bounds: default_all_resource_bounds,
        sender_address: account_address,
    });
    let execution_flags = ExecutionFlags { only_query, ..Default::default() };
    let invoke_tx = AccountTransaction { tx, execution_flags };

    let tx_execution_info = invoke_tx.execute(&mut state, &block_context).unwrap();
    assert_eq!(tx_execution_info.revert_error, None);
}

#[rstest]
fn test_l1_handler(#[values(false, true)] use_kzg_da: bool) {
    let gas_mode = GasVectorComputationMode::All;
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let block_context = &BlockContext::create_for_account_testing_with_kzg(use_kzg_da);
    let chain_info = &block_context.chain_info;
    let state = &mut test_state(chain_info, BALANCE, &[(test_contract, 1)]);
    let contract_address = test_contract.get_instance_address(0);
    let versioned_constants = &block_context.versioned_constants;
    let tx = l1handler_tx(Fee(1), contract_address);
    let calldata = tx.tx.calldata.clone();
    let key = calldata.0[1];
    let value = calldata.0[2];
    let payload_size = tx.payload_size();
    let mut actual_execution_info = tx.execute(state, block_context).unwrap();

    // Build the expected call info.
    let accessed_storage_key = StorageKey::try_from(key).unwrap();
    let expected_call_info = CallInfo {
        call: CallEntryPoint {
            class_hash: Some(test_contract.get_class_hash()),
            code_address: None,
            entry_point_type: EntryPointType::L1Handler,
            entry_point_selector: selector_from_name("l1_handler_set_value"),
            calldata: calldata.clone(),
            storage_address: contract_address,
            caller_address: ContractAddress::default(),
            call_type: CallType::Call,
            initial_gas: block_context
                .versioned_constants
                .os_constants
                .l1_handler_max_amount_bounds
                .l2_gas
                .0,
        },
        execution: CallExecution {
            retdata: Retdata(vec![value]),
            gas_consumed: 0, // Regression-tested explicitly.
            ..Default::default()
        },
        storage_access_tracker: StorageAccessTracker {
            accessed_storage_keys: HashSet::from_iter(vec![accessed_storage_key]),
            ..Default::default()
        },
        tracked_resource: test_contract
            .get_runnable_class()
            .tracked_resource(&versioned_constants.min_sierra_version_for_sierra_gas, None),
        builtin_counters: HashMap::from([(BuiltinName::range_check, 6)]),
        ..Default::default()
    };

    let expected_da_gas = match use_kzg_da {
        true => GasVector::from_l1_data_gas(128_u32.into()),
        false => GasVector::from_l1_gas(1652_u32.into()),
    };

    let state_changes_count = StateChangesCount {
        n_storage_updates: 1,
        n_modified_contracts: 1,
        ..StateChangesCount::default()
    };

    let mut expected_os_execution_resources = ExecutionResources {
        builtin_instance_counter: HashMap::from([
            (BuiltinName::pedersen, 11 + payload_size),
            (
                BuiltinName::range_check,
                get_tx_resources(TransactionType::L1Handler).builtin_instance_counter
                    [&BuiltinName::range_check],
            ),
        ]),
        n_steps: get_tx_resources(TransactionType::L1Handler).n_steps + 13,
        n_memory_holes: 0,
    };

    add_kzg_da_resources_to_resources_mapping(
        &mut expected_os_execution_resources,
        &state_changes_count,
        versioned_constants,
        use_kzg_da,
    );

    // Copy StarknetResources from actual resources and assert gas usage calculation is correct.
    let expected_tx_resources = TransactionResources {
        starknet_resources: actual_execution_info.receipt.resources.starknet_resources.clone(),
        computation: ComputationResources {
            os_vm_resources: expected_os_execution_resources,
            sierra_gas: GasAmount(0), // Regression-tested explicitly.
            ..Default::default()
        },
    };

    // Regression-test the execution gas consumed.
    // First, compute that actual gas vectors (before nullifying the gas fields on the actual
    // execution info) to get the correct values.
    let actual_gas_vector = actual_execution_info.receipt.resources.to_gas_vector(
        versioned_constants,
        use_kzg_da,
        &gas_mode,
    );

    // Regression-test the gas consumed, and then set to zero to compare the rest of the resources.
    let expected_gas = expect![[r#"
        15850
    "#]];
    expected_gas.assert_debug_eq(&actual_execution_info.receipt.resources.computation.sierra_gas.0);
    actual_execution_info.receipt.resources.computation.sierra_gas.0 = 0;
    assert_eq!(actual_execution_info.receipt.resources, expected_tx_resources);

    match use_kzg_da {
        true => expect![[r#"
            GasVector {
                l1_gas: GasAmount(
                    16023,
                ),
                l1_data_gas: GasAmount(
                    160,
                ),
                l2_gas: GasAmount(
                    200875,
                ),
            }
        "#]]
        .assert_debug_eq(&actual_gas_vector),
        false => expect![[r#"
            GasVector {
                l1_gas: GasAmount(
                    18226,
                ),
                l1_data_gas: GasAmount(
                    0,
                ),
                l2_gas: GasAmount(
                    149975,
                ),
            }
        "#]]
        .assert_debug_eq(&actual_gas_vector),
    };
    assert_eq!(use_kzg_da, actual_gas_vector.l1_data_gas.0 > 0);

    // Build the expected execution info.
    let expected_execution_info = TransactionExecutionInfo {
        validate_call_info: None,
        execute_call_info: Some(expected_call_info),
        fee_transfer_call_info: None,
        receipt: TransactionReceipt {
            fee: Fee(0),
            da_gas: expected_da_gas,
            resources: expected_tx_resources.clone(),
            // Gas vector was already tested; set the expected to the actual.
            gas: actual_gas_vector,
        },
        revert_error: None,
    };

    // Check the actual returned execution info.
    // First, regression-test the execution gas consumed, and set to zero after testing to easily
    // compare the rest of the fields.
    let mut actual_execute_call_info = actual_execution_info.execute_call_info.unwrap();
    expected_gas.assert_debug_eq(&actual_execute_call_info.execution.gas_consumed);
    actual_execute_call_info.execution.gas_consumed = 0;
    actual_execution_info.execute_call_info = Some(actual_execute_call_info);
    assert_eq!(actual_execution_info, expected_execution_info);

    // Check the state changes.
    assert_eq!(
        state.get_storage_at(contract_address, StorageKey::try_from(key).unwrap(),).unwrap(),
        value,
    );
    // Negative flow: transaction execution failed.
    let mut tx = l1handler_tx(Fee(1), contract_address);
    let arbitrary_entry_point_selector = selector_from_name("arbitrary");
    tx.tx.entry_point_selector = arbitrary_entry_point_selector;

    let execution_info = tx.execute(state, block_context).unwrap();
    let mut error_stack_segments = assert_matches!(
        execution_info,
        TransactionExecutionInfo {
            validate_call_info: None,
            execute_call_info: None,
            fee_transfer_call_info : None,
            revert_error: Some(
                RevertError::Execution(ErrorStack { header: ErrorStackHeader::Execution, stack })
            ),
            receipt: TransactionReceipt { fee: Fee(0), .. },
        }
        => stack
    );
    assert_eq!(error_stack_segments.len(), 2);
    let cairo_1_revert_summery =
        error_stack_segments.pop().expect("Expected at least two elements in the error stack");
    let entry_point_error_frame =
        error_stack_segments.pop().expect("Expected at least two elements in the error stack");
    assert_matches!(
        cairo_1_revert_summery,
        ErrorStackSegment::Cairo1RevertSummary(Cairo1RevertSummary { last_retdata, .. })
        if last_retdata == retdata!(ascii_as_felt("ENTRYPOINT_NOT_FOUND").unwrap())
    );
    assert_matches!(
        entry_point_error_frame,
        ErrorStackSegment::EntryPoint(EntryPointErrorFrame { selector: Some(selector), .. })
        if selector == arbitrary_entry_point_selector
    );

    // Negative flow: not enough fee paid on L1.

    // set the storage back to 0, so the fee will also include the storage write.
    // TODO(Meshi, 15/6/2024): change the l1_handler_set_value cairo function to
    // always update the storage instead.
    state.set_storage_at(contract_address, StorageKey::try_from(key).unwrap(), Felt::ZERO).unwrap();
    let tx_no_fee = l1handler_tx(Fee(0), contract_address);
    let error = tx_no_fee.execute(state, block_context).unwrap_err(); // Do not charge fee as L1Handler's resource bounds (/max fee) is 0.
    // Today, we check that the paid_fee is positive, no matter what was the actual fee.
    let tip = block_context.to_tx_context(&tx_no_fee).effective_tip();
    let expected_actual_fee =
        get_fee_by_gas_vector(&block_context.block_info, actual_gas_vector, &FeeType::Eth, tip);

    assert_matches!(
        error,
        TransactionExecutionError::TransactionFeeError(boxed_fee_error)
        if matches!(
            *boxed_fee_error,
            TransactionFeeError::InsufficientFee { paid_fee, actual_fee }
            if paid_fee == Fee(0) && actual_fee == expected_actual_fee
        )
    );
}

#[rstest]
#[case(L1Gas, GasAmount(1))]
// Sufficient to pass execution (enough gas to run the transaction), but fails post-execution
// resource bounds check.
#[case(L2Gas, GasAmount(200000))]
#[case(L1DataGas, GasAmount(1))]
fn test_l1_handler_resource_bounds(#[case] resource: Resource, #[case] new_bound: GasAmount) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));

    // Set to true to ensure L1 data gas is non-zero.
    let use_kzg_da = true;

    let mut block_context = BlockContext::create_for_account_testing_with_kzg(use_kzg_da);
    let chain_info = block_context.chain_info.clone();
    let mut state = test_state(&chain_info, BALANCE, &[(test_contract, 1)]);
    let contract_address = test_contract.get_instance_address(0);

    // Modify the resource bound for the tested resource.
    let os_constants = Arc::make_mut(&mut block_context.versioned_constants.os_constants);
    match resource {
        L1Gas => os_constants.l1_handler_max_amount_bounds.l1_gas = new_bound,
        L2Gas => os_constants.l1_handler_max_amount_bounds.l2_gas = new_bound,
        L1DataGas => os_constants.l1_handler_max_amount_bounds.l1_data_gas = new_bound,
    }

    let tx = l1handler_tx(Fee(1), contract_address);

    let execution_info = tx.execute(&mut state, &block_context).unwrap();

    assert_matches!(
        execution_info,
        TransactionExecutionInfo {
            validate_call_info: None,
            execute_call_info: None,
            fee_transfer_call_info: None,
            revert_error: Some(RevertError::PostExecution(FeeCheckError::MaxGasAmountExceeded {
                resource: r,
                max_amount,
                actual_amount
            })),
            // TODO(Arni): consider checking other fields of the receipt.
            receipt: TransactionReceipt { fee, .. },
        } if r == resource && new_bound == max_amount && actual_amount > max_amount && fee == Fee(0)
    );
}

#[rstest]
fn test_execute_tx_with_invalid_tx_version(
    block_context: BlockContext,
    default_all_resource_bounds: ValidResourceBounds,
) {
    let TestInitData { mut state, account_address, contract_address, .. } =
        create_test_init_data(&block_context.chain_info, CairoVersion::Cairo0);
    let invalid_version = 12345_u64;
    let calldata = create_calldata(contract_address, "test_tx_version", &[felt!(invalid_version)]);
    let account_tx = invoke_tx_with_default_flags(invoke_tx_args! {
        resource_bounds: default_all_resource_bounds,
        sender_address: account_address,
        calldata,
    });

    let execution_info = account_tx.execute(&mut state, &block_context).unwrap();
    assert!(
        execution_info
            .revert_error
            .unwrap()
            .to_string()
            .contains(format!("ASSERT_EQ instruction failed: {invalid_version} != 3.").as_str())
    );
}

fn max_n_emitted_events() -> usize {
    VERSIONED_CONSTANTS.tx_event_limits.max_n_emitted_events
}

fn max_event_keys() -> usize {
    VERSIONED_CONSTANTS.tx_event_limits.max_keys_length
}

fn max_event_data() -> usize {
    VERSIONED_CONSTANTS.tx_event_limits.max_data_length
}

#[apply(cairo_version)]
#[case::positive_flow(
    vec![felt!(1_u16); max_event_keys()],
    vec![felt!(2_u16); max_event_data()],
    max_n_emitted_events(),
    None)]
#[case::exceeds_max_number_of_events(
    vec![felt!(1_u16)],
    vec![felt!(2_u16)],
    max_n_emitted_events() + 1,
    Some(EmitEventError::ExceedsMaxNumberOfEmittedEvents {
        n_emitted_events: max_n_emitted_events() + 1,
        max_n_emitted_events: max_n_emitted_events(),
    }))]
#[case::exceeds_max_number_of_keys(
    vec![felt!(3_u16); max_event_keys() + 1],
    vec![felt!(4_u16)],
    1,
    Some(EmitEventError::ExceedsMaxKeysLength{
        keys_length: max_event_keys() + 1,
        max_keys_length: max_event_keys(),
    }))]
#[case::exceeds_data_length(
    vec![felt!(5_u16)],
    vec![felt!(6_u16); max_event_data() + 1],
    1,
    Some(EmitEventError::ExceedsMaxDataLength{
        data_length: max_event_data() + 1,
        max_data_length: max_event_data(),
    }))]
fn test_emit_event_exceeds_limit(
    block_context: BlockContext,
    default_all_resource_bounds: ValidResourceBounds,
    #[case] event_keys: Vec<Felt>,
    #[case] event_data: Vec<Felt>,
    #[case] n_emitted_events: usize,
    #[case] expected_error: Option<EmitEventError>,
    cairo_version: CairoVersion,
) {
    let TestInitData { mut state, account_address, contract_address, .. } =
        create_test_init_data(&block_context.chain_info, cairo_version);

    let calldata = [
        vec![felt!(u16::try_from(n_emitted_events).expect("Failed to convert usize to u16."))]
            .to_owned(),
        vec![felt!(u16::try_from(event_keys.len()).expect("Failed to convert usize to u16."))],
        event_keys.clone(),
        vec![felt!(u16::try_from(event_data.len()).expect("Failed to convert usize to u16."))],
        event_data.clone(),
    ]
    .concat();
    let execute_calldata = Calldata(
        [
            vec![contract_address.into()],
            vec![selector_from_name("test_emit_events").0],
            vec![felt!(u16::try_from(calldata.len()).expect("Failed to convert usize to u16."))],
            calldata.clone(),
        ]
        .concat()
        .into(),
    );

    let account_tx = invoke_tx_with_default_flags(invoke_tx_args! {
        sender_address: account_address,
        calldata: execute_calldata,
        resource_bounds: default_all_resource_bounds,
        nonce: nonce!(0_u8),
    });
    let execution_info = account_tx.execute(&mut state, &block_context).unwrap();
    match &expected_error {
        Some(expected_error) => {
            let error_string = execution_info.revert_error.unwrap().to_string();
            assert!(error_string.contains(&format!("{expected_error}")));
        }
        None => {
            assert!(!execution_info.is_reverted());
        }
    }
}

#[test]
fn test_balance_print() {
    let int = balance_to_big_uint(&Felt::from(16_u64), &Felt::from(1_u64));
    assert!(format!("{int}") == (BigUint::from(u128::MAX) + BigUint::from(17_u128)).to_string());
}

#[apply(two_cairo_versions)]
#[case::small_user_bounds(invoke_tx_args! {
    version: TransactionVersion::THREE,
    resource_bounds: create_gas_amount_bounds_with_default_price(
        GasVector{ l1_gas: GasAmount(1652), l2_gas: GasAmount(654321), l1_data_gas: GasAmount(0) }
    ),
})]
#[case::user_bounds_between_validate_and_execute(invoke_tx_args! {
    version: TransactionVersion::THREE,
    resource_bounds: create_gas_amount_bounds_with_default_price(
        GasVector{
            l1_gas: GasAmount(1652),
            l2_gas: versioned_constants.os_constants.validate_max_sierra_gas + GasAmount(1234567),
            l1_data_gas: GasAmount(0),
        }
    ),
})]
#[case::large_user_bounds(invoke_tx_args! {
    version: TransactionVersion::THREE,
    resource_bounds: default_all_resource_bounds(),
})]
#[case::l1_user_bounds(invoke_tx_args! {
    version: TransactionVersion::THREE,
    resource_bounds: default_l1_resource_bounds(),
})]
#[case::deprecated_tx_version(invoke_tx_args! {
    version: TransactionVersion::ONE,
    max_fee: Fee(1000000000000000),
})]
fn test_invoke_max_sierra_gas_validate_execute(
    block_context: BlockContext,
    versioned_constants: VersionedConstants,
    #[case] tx_args: InvokeTxArgs,
    cairo_version1: CairoVersion,
    cairo_version2: CairoVersion,
) {
    let account_cairo_version = cairo_version1;
    let contract_cairo_version = cairo_version2;
    let account_contract = FeatureContract::AccountWithoutValidations(account_cairo_version);
    let test_contract = FeatureContract::TestContract(contract_cairo_version);
    let chain_info = &block_context.chain_info;
    let state = &mut test_state(chain_info, BALANCE, &[(account_contract, 1), (test_contract, 1)]);
    let test_contract_address = test_contract.get_instance_address(0);
    let account_contract_address = account_contract.get_instance_address(0);
    let calldata = create_calldata(test_contract_address, "recurse", &[felt!(10_u8)]);
    let invoke_tx = invoke_tx_with_default_flags(invoke_tx_args! {
        sender_address: account_contract_address, calldata: Calldata(Arc::clone(&calldata.0)), .. tx_args
    });
    let user_initial_gas = if tx_args.version == TransactionVersion::THREE {
        user_initial_gas_from_bounds(tx_args.resource_bounds, Some(&block_context))
    } else {
        initial_gas_amount_from_block_context(Some(&block_context))
    };

    let actual_execution_info = invoke_tx.execute(state, &block_context).unwrap();

    let account_tracked_resource = account_contract
        .get_runnable_class()
        .tracked_resource(&versioned_constants.min_sierra_version_for_sierra_gas, None);

    let contract_tracked_resource = test_contract.get_runnable_class().tracked_resource(
        &versioned_constants.min_sierra_version_for_sierra_gas,
        Some(&account_tracked_resource),
    );

    let actual_validate_initial_gas =
        actual_execution_info.validate_call_info.as_ref().unwrap().call.initial_gas;
    let expected_validate_initial_gas = match account_tracked_resource {
        TrackedResource::CairoSteps => VERSIONED_CONSTANTS.infinite_gas_for_vm_mode(),
        TrackedResource::SierraGas => {
            versioned_constants.os_constants.validate_max_sierra_gas.min(user_initial_gas).0
        }
    };

    assert_eq!(actual_validate_initial_gas, expected_validate_initial_gas);

    let actual_execute_initial_gas =
        actual_execution_info.execute_call_info.as_ref().unwrap().call.initial_gas;
    let expected_execute_initial_gas = match account_tracked_resource {
        TrackedResource::CairoSteps => VERSIONED_CONSTANTS.infinite_gas_for_vm_mode(),
        TrackedResource::SierraGas => {
            versioned_constants
                .os_constants
                .execute_max_sierra_gas
                .min(
                    user_initial_gas
                        - GasAmount(
                            actual_execution_info
                                .validate_call_info
                                .as_ref()
                                .unwrap()
                                .execution
                                .gas_consumed,
                        ),
                )
                .0
        }
    };
    assert_eq!(actual_execute_initial_gas, expected_execute_initial_gas);

    let actual_inner_call_initial_gas =
        actual_execution_info.execute_call_info.as_ref().unwrap().inner_calls[0].call.initial_gas;
    if contract_tracked_resource == TrackedResource::SierraGas {
        assert!(actual_inner_call_initial_gas < actual_execute_initial_gas);
        assert!(
            actual_inner_call_initial_gas
                > actual_execute_initial_gas
                    - actual_execution_info
                        .execute_call_info
                        .as_ref()
                        .unwrap()
                        .execution
                        .gas_consumed
        );
    } else {
        assert_eq!(actual_inner_call_initial_gas, versioned_constants.infinite_gas_for_vm_mode());
    };
}

#[apply(cairo_version)]
#[case::small_user_bounds(deploy_account_tx_args! {
    version: TransactionVersion::THREE,
    resource_bounds: create_gas_amount_bounds_with_default_price(
        GasVector{ l1_gas: GasAmount(2203), l2_gas: GasAmount(654321), l1_data_gas: GasAmount(0) }
    ),
})]
#[case::user_bounds_between_validate_and_execute(deploy_account_tx_args! {
    version: TransactionVersion::THREE,
    resource_bounds: create_gas_amount_bounds_with_default_price(
        GasVector{
            l1_gas: GasAmount(2203),
            l2_gas: versioned_constants.os_constants.validate_max_sierra_gas + GasAmount(1234567),
            l1_data_gas: GasAmount(0),
        }
    ),
})]
#[case::large_user_bounds(deploy_account_tx_args! {
    version: TransactionVersion::THREE,
    resource_bounds: default_all_resource_bounds(),
})]
#[case::l1_user_bounds(deploy_account_tx_args! {
    version: TransactionVersion::THREE,
    resource_bounds: default_l1_resource_bounds(),
})]
#[case::deprecated_tx_version(deploy_account_tx_args! {
    version: TransactionVersion::ONE,
    max_fee: Fee(1000000000000000),
})]
fn test_deploy_max_sierra_gas_validate_execute(
    block_context: BlockContext,
    versioned_constants: VersionedConstants,
    cairo_version: CairoVersion,
    #[case] tx_args: DeployAccountTxArgs,
) {
    let chain_info = &block_context.chain_info;
    let account = FeatureContract::AccountWithoutValidations(cairo_version);
    let account_class_hash = account.get_class_hash();
    let state = &mut test_state(chain_info, BALANCE, &[(account, 1)]);
    let deploy_account = AccountTransaction::new_with_default_flags(executable_deploy_account_tx(
        deploy_account_tx_args! {
            class_hash: account_class_hash,
            .. tx_args
        },
    ));

    // Extract deploy account transaction fields for testing, as it is consumed when creating an
    // account transaction.
    let user_initial_gas = if tx_args.version == TransactionVersion::THREE {
        user_initial_gas_from_bounds(tx_args.resource_bounds, Some(&block_context))
    } else {
        initial_gas_amount_from_block_context(Some(&block_context))
    };

    // Update the balance of the about to be deployed account contract in the erc20 contract, so it
    // can pay for the transaction execution.
    fund_account(chain_info, deploy_account.tx.contract_address(), BALANCE, &mut state.state);

    let account_tracked_resource = account
        .get_runnable_class()
        .tracked_resource(&versioned_constants.min_sierra_version_for_sierra_gas, None);

    let actual_execution_info = deploy_account.execute(state, &block_context).unwrap();

    let actual_execute_initial_gas =
        actual_execution_info.execute_call_info.as_ref().unwrap().call.initial_gas;
    let expected_execute_initial_gas = match account_tracked_resource {
        TrackedResource::CairoSteps => VERSIONED_CONSTANTS.infinite_gas_for_vm_mode(),
        TrackedResource::SierraGas => {
            versioned_constants.os_constants.validate_max_sierra_gas.min(user_initial_gas).0
        }
    };
    assert_eq!(actual_execute_initial_gas, expected_execute_initial_gas);

    let actual_validate_initial_gas =
        actual_execution_info.validate_call_info.as_ref().unwrap().call.initial_gas;
    let expected_validate_initial_gas = match account_tracked_resource {
        TrackedResource::CairoSteps => VERSIONED_CONSTANTS.infinite_gas_for_vm_mode(),
        TrackedResource::SierraGas => {
            versioned_constants
                .os_constants
                .validate_max_sierra_gas
                .min(
                    user_initial_gas
                        - GasAmount(
                            actual_execution_info
                                .execute_call_info
                                .as_ref()
                                .unwrap()
                                .execution
                                .gas_consumed,
                        ),
                )
                .0
        }
    };
    assert_eq!(actual_validate_initial_gas, expected_validate_initial_gas);
}
