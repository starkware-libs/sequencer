use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock};

use assert_matches::assert_matches;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use num_bigint::BigUint;
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use starknet_api::abi::abi_utils::{
    get_fee_token_var_address,
    get_storage_var_address,
    selector_from_name,
};
use starknet_api::abi::constants::CONSTRUCTOR_ENTRY_POINT_NAME;
use starknet_api::block::{FeeType, GasPriceVector};
use starknet_api::contract_class::EntryPointType;
use starknet_api::core::{ChainId, ClassHash, ContractAddress, EthAddress, Nonce};
use starknet_api::executable_transaction::AccountTransaction as ApiExecutableTransaction;
use starknet_api::execution_resources::{GasAmount, GasVector};
use starknet_api::state::StorageKey;
use starknet_api::test_utils::invoke::InvokeTxArgs;
use starknet_api::test_utils::NonceManager;
use starknet_api::transaction::fields::Resource::{L1DataGas, L1Gas, L2Gas};
use starknet_api::transaction::fields::{
    AllResourceBounds,
    Calldata,
    Fee,
    GasVectorComputationMode,
    Resource,
    ResourceBounds,
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
use strum::IntoEnumIterator;

use crate::context::{BlockContext, ChainInfo, FeeTokenAddresses, TransactionContext};
use crate::execution::call_info::{
    CallExecution,
    CallInfo,
    ChargedResources,
    ExecutionSummary,
    MessageToL1,
    OrderedEvent,
    OrderedL2ToL1Message,
    Retdata,
};
use crate::execution::contract_class::TrackedResource;
use crate::execution::entry_point::{CallEntryPoint, CallType};
use crate::execution::errors::{ConstructorEntryPointExecutionError, EntryPointExecutionError};
use crate::execution::syscalls::hint_processor::EmitEventError;
use crate::execution::syscalls::SyscallSelector;
use crate::fee::fee_utils::{balance_to_big_uint, get_fee_by_gas_vector};
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
use crate::test_utils::contracts::FeatureContract;
use crate::test_utils::declare::declare_tx;
use crate::test_utils::deploy_account::deploy_account_tx;
use crate::test_utils::dict_state_reader::DictStateReader;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::invoke::invoke_tx;
use crate::test_utils::l1_handler::l1handler_tx;
use crate::test_utils::prices::Prices;
use crate::test_utils::{
    create_calldata,
    create_trivial_calldata,
    get_syscall_resources,
    get_tx_resources,
    test_erc20_sequencer_balance_key,
    CairoVersion,
    SaltManager,
    BALANCE,
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
use crate::transaction::account_transaction::{AccountTransaction, ExecutionFlags};
use crate::transaction::errors::{
    TransactionExecutionError,
    TransactionFeeError,
    TransactionPreValidationError,
};
use crate::transaction::objects::{
    HasRelatedFeeType,
    TransactionExecutionInfo,
    TransactionInfo,
    TransactionInfoCreator,
};
use crate::transaction::test_utils::{
    account_invoke_tx,
    block_context,
    calculate_class_info_for_testing,
    create_account_tx_for_validate_test,
    create_account_tx_for_validate_test_nonce_0,
    create_all_resource_bounds,
    default_all_resource_bounds,
    default_l1_resource_bounds,
    l1_resource_bounds,
    FaultyAccountTxCreatorArgs,
    CALL_CONTRACT,
    GET_BLOCK_HASH,
    GET_BLOCK_NUMBER,
    GET_BLOCK_TIMESTAMP,
    GET_EXECUTION_INFO,
    GET_SEQUENCER_ADDRESS,
    INVALID,
    VALID,
};
use crate::transaction::transaction_types::TransactionType;
use crate::transaction::transactions::ExecutableTransaction;
use crate::versioned_constants::{AllocationCost, VersionedConstants};
use crate::{
    check_tx_execution_error_for_custom_hint,
    check_tx_execution_error_for_invalid_scenario,
    retdata,
};

static VERSIONED_CONSTANTS: LazyLock<VersionedConstants> =
    LazyLock::new(VersionedConstants::create_for_testing);

#[fixture]
fn default_initial_gas_cost() -> u64 {
    VERSIONED_CONSTANTS.default_initial_gas_cost()
}

#[fixture]
fn versioned_constants_for_account_testing() -> VersionedConstants {
    VERSIONED_CONSTANTS.clone()
}

struct ExpectedResultTestInvokeTx {
    resources: ExecutionResources,
    validate_gas_consumed: u64,
    execute_gas_consumed: u64,
    inner_call_initial_gas: u64,
}

fn user_initial_gas_from_bounds(bounds: ValidResourceBounds) -> Option<GasAmount> {
    match bounds {
        ValidResourceBounds::L1Gas(_) => None,
        ValidResourceBounds::AllResources(bounds) => Some(bounds.l2_gas.max_amount),
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
        CairoVersion::Cairo1 => retdata!(*constants::VALIDATE_RETDATA),
        #[cfg(feature = "cairo_native")]
        CairoVersion::Native => retdata!(*constants::VALIDATE_RETDATA),
    };
    // Extra range check in regular (invoke) validate call, due to passing the calldata as an array.
    let n_range_checks = match cairo_version {
        CairoVersion::Cairo0 => {
            usize::from(entry_point_selector_name == constants::VALIDATE_ENTRY_POINT_NAME)
        }
        CairoVersion::Cairo1 => {
            if entry_point_selector_name == constants::VALIDATE_ENTRY_POINT_NAME { 7 } else { 2 }
        }
        #[cfg(feature = "cairo_native")]
        CairoVersion::Native => {
            if entry_point_selector_name == constants::VALIDATE_ENTRY_POINT_NAME { 7 } else { 2 }
        }
    };
    let n_steps = match (entry_point_selector_name, cairo_version) {
        (constants::VALIDATE_DEPLOY_ENTRY_POINT_NAME, CairoVersion::Cairo0) => 13_usize,
        (constants::VALIDATE_DEPLOY_ENTRY_POINT_NAME, CairoVersion::Cairo1) => 32_usize,
        (constants::VALIDATE_DECLARE_ENTRY_POINT_NAME, CairoVersion::Cairo0) => 12_usize,
        (constants::VALIDATE_DECLARE_ENTRY_POINT_NAME, CairoVersion::Cairo1) => 28_usize,
        (constants::VALIDATE_ENTRY_POINT_NAME, CairoVersion::Cairo0) => 21_usize,
        (constants::VALIDATE_ENTRY_POINT_NAME, CairoVersion::Cairo1) => 100_usize,
        (selector, _) => panic!("Selector {selector} is not a known validate selector."),
    };
    let resources = ExecutionResources {
        n_steps,
        n_memory_holes: 0,
        builtin_instance_counter: HashMap::from([(BuiltinName::range_check, n_range_checks)]),
    }
    .filter_unused_builtins();
    let initial_gas = user_initial_gas.unwrap_or(GasAmount(default_initial_gas_cost())).0;

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
        charged_resources: ChargedResources::from_execution_resources(resources),
        execution: CallExecution { retdata, gas_consumed, ..Default::default() },
        tracked_resource,
        ..Default::default()
    })
}

fn expected_fee_transfer_call_info(
    tx_context: &TransactionContext,
    account_address: ContractAddress,
    actual_fee: Fee,
    expected_fee_token_class_hash: ClassHash,
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
    Some(CallInfo {
        call: expected_fee_transfer_call,
        execution: CallExecution {
            retdata: retdata![felt!(constants::FELT_TRUE)],
            events: vec![expected_fee_transfer_event],
            ..Default::default()
        },
        charged_resources: ChargedResources::from_execution_resources(
            Prices::FeeTransfer(account_address, *fee_type).into(),
        ),
        // We read sender and recipient balance - Uint256(BALANCE, 0) then Uint256(0, 0).
        storage_read_values: vec![felt!(BALANCE.0), felt!(0_u8), felt!(0_u8), felt!(0_u8)],
        accessed_storage_keys: HashSet::from_iter(vec![
            sender_balance_key_low,
            sender_balance_key_high,
            sequencer_balance_key_low,
            sequencer_balance_key_high,
        ]),
        ..Default::default()
    })
}

fn get_expected_cairo_resources(
    versioned_constants: &VersionedConstants,
    tx_type: TransactionType,
    starknet_resources: &StarknetResources,
    call_infos: Vec<&Option<CallInfo>>,
) -> ExecutionResources {
    let mut expected_cairo_resources =
        versioned_constants.get_additional_os_tx_resources(tx_type, starknet_resources, false);
    for call_info in call_infos {
        if let Some(call_info) = &call_info {
            expected_cairo_resources += &call_info.charged_resources.vm_resources
        };
    }

    expected_cairo_resources
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

#[rstest]
#[case::with_cairo0_account(
    ExpectedResultTestInvokeTx{
        resources: &get_syscall_resources(SyscallSelector::CallContract) + &ExecutionResources {
            n_steps: 62,
            n_memory_holes:  0,
            builtin_instance_counter: HashMap::from([(BuiltinName::range_check, 1)]),
        },
        validate_gas_consumed: 0,
        execute_gas_consumed: 0,
        inner_call_initial_gas: versioned_constants_for_account_testing().default_initial_gas_cost(),
    },
    CairoVersion::Cairo0)]
#[case::with_cairo1_account(
    ExpectedResultTestInvokeTx{
        resources: &get_syscall_resources(SyscallSelector::CallContract) + &ExecutionResources {
            n_steps: 207,
            n_memory_holes: 0,
            builtin_instance_counter: HashMap::from([(BuiltinName::range_check, 8)]),
        },
        validate_gas_consumed: 4740, // The gas consumption results from parsing the input
            // arguments.
        execute_gas_consumed: 162080,
        inner_call_initial_gas: versioned_constants_for_account_testing().default_initial_gas_cost(),
    },
    CairoVersion::Cairo1)]
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
    let invoke_tx = AccountTransaction::new(invoke_tx(invoke_tx_args! {
        sender_address: account_contract_address,
        calldata: Calldata(Arc::clone(&calldata.0)),
        resource_bounds,
    }));

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

    let tracked_resource = account_contract.get_runnable_class().tracked_resource(
        &versioned_constants.min_compiler_version_for_sierra_gas,
        tx_context.tx_info.gas_mode(),
    );
    if tracked_resource == TrackedResource::CairoSteps {
        // In CairoSteps mode, the initial gas is set to the default once before the validate call.
        expected_arguments.inner_call_initial_gas -=
            expected_arguments.validate_gas_consumed + expected_arguments.execute_gas_consumed
    }

    // Build expected validate call info.
    let expected_account_class_hash = account_contract.get_class_hash();
    let expected_validate_call_info = expected_validate_call_info(
        expected_account_class_hash,
        constants::VALIDATE_ENTRY_POINT_NAME,
        expected_arguments.validate_gas_consumed,
        calldata,
        sender_address,
        account_cairo_version,
        tracked_resource,
        user_initial_gas_from_bounds(resource_bounds),
    );

    // Build expected execute call info.
    let expected_return_result_calldata = vec![felt!(2_u8)];
    let expected_return_result_call = CallEntryPoint {
        entry_point_selector: selector_from_name("return_result"),
        class_hash: Some(test_contract.get_class_hash()),
        code_address: Some(test_contract_address),
        entry_point_type: EntryPointType::External,
        calldata: Calldata(expected_return_result_calldata.clone().into()),
        storage_address: test_contract_address,
        caller_address: sender_address,
        call_type: CallType::Call,
        initial_gas: expected_arguments.inner_call_initial_gas,
    };
    let expected_validated_call = expected_validate_call_info.as_ref().unwrap().call.clone();
    let expected_execute_call = CallEntryPoint {
        entry_point_selector: selector_from_name(constants::EXECUTE_ENTRY_POINT_NAME),
        initial_gas: expected_validated_call.initial_gas - expected_arguments.validate_gas_consumed,
        ..expected_validated_call
    };
    let expected_return_result_retdata = Retdata(expected_return_result_calldata);
    let expected_execute_call_info = Some(CallInfo {
        call: expected_execute_call,
        execution: CallExecution {
            retdata: Retdata(expected_return_result_retdata.0.clone()),
            gas_consumed: expected_arguments.execute_gas_consumed,
            ..Default::default()
        },
        charged_resources: ChargedResources::from_execution_resources(expected_arguments.resources),
        inner_calls: vec![CallInfo {
            call: expected_return_result_call,
            execution: CallExecution::from_retdata(expected_return_result_retdata),
            charged_resources: ChargedResources::from_execution_resources(ExecutionResources {
                n_steps: 23,
                n_memory_holes: 0,
                ..Default::default()
            }),
            ..Default::default()
        }],
        tracked_resource,
        ..Default::default()
    });

    // Build expected fee transfer call info.
    let fee_type = &tx_context.tx_info.fee_type();
    let expected_actual_fee = actual_execution_info.receipt.fee;
    let expected_fee_transfer_call_info = expected_fee_transfer_call_info(
        &tx_context,
        sender_address,
        expected_actual_fee,
        FeatureContract::ERC20(CairoVersion::Cairo0).get_class_hash(),
    );

    let da_gas = starknet_resources.state.da_gas_vector(use_kzg_da);

    let expected_cairo_resources = get_expected_cairo_resources(
        versioned_constants,
        TransactionType::InvokeFunction,
        &starknet_resources,
        vec![&expected_validate_call_info, &expected_execute_call_info],
    );
    let mut expected_actual_resources = TransactionResources {
        starknet_resources,
        computation: ComputationResources {
            vm_resources: expected_cairo_resources,
            ..Default::default()
        },
    };

    add_kzg_da_resources_to_resources_mapping(
        &mut expected_actual_resources.computation.vm_resources,
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
fn test_invoke_tx_advanced_operations(
    block_context: BlockContext,
    default_all_resource_bounds: ValidResourceBounds,
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1)] cairo_version: CairoVersion,
) {
    let block_context = &block_context;
    let account = FeatureContract::AccountWithoutValidations(cairo_version);
    let test_contract = FeatureContract::TestContract(cairo_version);
    let state =
        &mut test_state(&block_context.chain_info, BALANCE, &[(account, 1), (test_contract, 1)]);
    let account_address = account.get_instance_address(0);
    let contract_address = test_contract.get_instance_address(0);
    let index = felt!(123_u32);
    let base_tx_args = invoke_tx_args! {
        resource_bounds: default_all_resource_bounds,
        sender_address: account_address,
    };

    // Invoke advance_counter function.
    let mut nonce_manager = NonceManager::default();
    let counter_diffs = [101_u32, 102_u32];
    let initial_counters = [felt!(counter_diffs[0]), felt!(counter_diffs[1])];
    let calldata_args = vec![index, initial_counters[0], initial_counters[1]];

    let account_tx = account_invoke_tx(invoke_tx_args! {
        nonce: nonce_manager.next(account_address),
        calldata:
            create_calldata(contract_address, "advance_counter", &calldata_args),
        ..base_tx_args.clone()
    });
    account_tx.execute(state, block_context).unwrap();

    let next_nonce = nonce_manager.next(account_address);
    let initial_ec_point = [Felt::ZERO, Felt::ZERO];
    verify_storage_after_invoke_advanced_operations(
        state,
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

    let account_tx = account_invoke_tx(invoke_tx_args! {
        nonce: next_nonce,
        calldata:
            create_calldata(contract_address, "call_xor_counters", &calldata_args),
        ..base_tx_args.clone()
    });
    account_tx.execute(state, block_context).unwrap();

    let expected_counters =
        [felt!(counter_diffs[0] ^ xor_values[0]), felt!(counter_diffs[1] ^ xor_values[1])];
    let next_nonce = nonce_manager.next(account_address);
    verify_storage_after_invoke_advanced_operations(
        state,
        contract_address,
        account_address,
        index,
        expected_counters,
        initial_ec_point,
        next_nonce,
    );

    // Invoke test_ec_op function.
    let account_tx = account_invoke_tx(invoke_tx_args! {
        nonce: next_nonce,
        calldata:
            create_calldata(contract_address, "test_ec_op", &[]),
        ..base_tx_args.clone()
    });
    account_tx.execute(state, block_context).unwrap();

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
        state,
        contract_address,
        account_address,
        index,
        expected_counters,
        expected_ec_point,
        next_nonce,
    );

    // Invoke add_signature_to_counters function.
    let signature_values = [Felt::from(200_u64), Felt::from(300_u64)];
    let signature = TransactionSignature(signature_values.into());

    let account_tx = account_invoke_tx(invoke_tx_args! {
        signature,
        nonce: next_nonce,
        calldata:
            create_calldata(contract_address, "add_signature_to_counters", &[index]),
        ..base_tx_args.clone()
    });
    account_tx.execute(state, block_context).unwrap();

    let expected_counters = [
        (expected_counters[0] + signature_values[0]),
        (expected_counters[1] + signature_values[1]),
    ];
    let next_nonce = nonce_manager.next(account_address);
    verify_storage_after_invoke_advanced_operations(
        state,
        contract_address,
        account_address,
        index,
        expected_counters,
        expected_ec_point,
        next_nonce,
    );

    // Invoke send_message function that send a message to L1.
    let to_address = Felt::from(85);
    let account_tx = account_invoke_tx(invoke_tx_args! {
        nonce: next_nonce,
        calldata:
            create_calldata(contract_address, "send_message", &[to_address]),
        ..base_tx_args
    });
    let execution_info = account_tx.execute(state, block_context).unwrap();
    let next_nonce = nonce_manager.next(account_address);
    verify_storage_after_invoke_advanced_operations(
        state,
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
            to_address: EthAddress::try_from(to_address).unwrap(),
            payload: L2ToL1Payload(vec![felt!(12_u32), felt!(34_u32)]),
        },
    };
    assert_eq!(
        expected_msg,
        execution_info.execute_call_info.unwrap().inner_calls[0].execution.l2_to_l1_messages[0]
    );
}

#[rstest]
#[case(TransactionVersion::ONE, FeeType::Eth)]
#[case(TransactionVersion::THREE, FeeType::Strk)]
fn test_state_get_fee_token_balance(
    block_context: BlockContext,
    #[case] tx_version: TransactionVersion,
    #[case] fee_type: FeeType,
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1)] account_version: CairoVersion,
) {
    let block_context = &block_context;
    let chain_info = &block_context.chain_info;
    let account = FeatureContract::AccountWithoutValidations(account_version);
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo0);
    let state = &mut test_state(chain_info, BALANCE, &[(account, 1), (test_contract, 1)]);
    let account_address = account.get_instance_address(0);
    let (mint_high, mint_low) = (felt!(54_u8), felt!(39_u8));
    let recipient_int = 10_u8;
    let recipient = felt!(recipient_int);
    let fee_token_address = chain_info.fee_token_address(&fee_type);

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
    let account_tx = account_invoke_tx(invoke_tx_args! {
        max_fee: MAX_FEE,
        resource_bounds: default_l1_resource_bounds(),
        sender_address: account_address,
        calldata: execute_calldata,
        version: tx_version,
        nonce: Nonce::default(),
    });
    account_tx.execute(state, block_context).unwrap();

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
                TransactionExecutionError::TransactionPreValidationError(
                    TransactionPreValidationError::TransactionFeeError(
                        TransactionFeeError::MaxFeeExceedsBalance{ max_fee, .. }))
                if max_fee == context.max_fee
            );
        }
        TransactionInfo::Current(context) => match context.resource_bounds {
            ValidResourceBounds::L1Gas(l1_bounds) => assert_matches!(
                tx_error,
                TransactionExecutionError::TransactionPreValidationError(
                    TransactionPreValidationError::TransactionFeeError(
                        TransactionFeeError::GasBoundsExceedBalance{
                            resource, max_amount, max_price, ..
                        }
                    )
                )
                if max_amount == l1_bounds.max_amount
                    && max_price == l1_bounds.max_price_per_unit
                    && resource == L1Gas
            ),
            ValidResourceBounds::AllResources(actual_bounds) => {
                assert_matches!(
                    tx_error,
                    TransactionExecutionError::TransactionPreValidationError(
                        TransactionPreValidationError::TransactionFeeError(
                            TransactionFeeError::ResourcesBoundsExceedBalance {
                                bounds: error_bounds, ..
                            }
                        )
                    )
                    if actual_bounds == error_bounds
                );
            }
        },
    };
}

#[rstest]
fn test_estimate_minimal_gas_vector(
    mut block_context: BlockContext,
    #[values(true, false)] use_kzg_da: bool,
    #[values(GasVectorComputationMode::NoL2Gas, GasVectorComputationMode::All)]
    gas_vector_computation_mode: GasVectorComputationMode,
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1)] account_cairo_version: CairoVersion,
) {
    block_context.block_info.use_kzg_da = use_kzg_da;
    let block_context = &block_context;
    let account_contract = FeatureContract::AccountWithoutValidations(account_cairo_version);
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo0);

    let valid_invoke_tx_args = invoke_tx_args! {
        sender_address: account_contract.get_instance_address(0),
        calldata: create_trivial_calldata(test_contract.get_instance_address(0)),
        max_fee: MAX_FEE
    };

    // The minimal gas estimate does not depend on tx version.
    let tx = &account_invoke_tx(valid_invoke_tx_args);
    let minimal_gas_vector =
        estimate_minimal_gas_vector(block_context, tx, &gas_vector_computation_mode);
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
fn test_max_fee_exceeds_balance(
    mut block_context: BlockContext,
    #[values(default_l1_resource_bounds(), default_all_resource_bounds())]
    resource_bounds: ValidResourceBounds,
    #[values(true, false)] use_kzg_da: bool,
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1)] account_cairo_version: CairoVersion,
) {
    block_context.block_info.use_kzg_da = use_kzg_da;
    let block_context = &block_context;
    let account_contract = FeatureContract::AccountWithoutValidations(account_cairo_version);
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo0);
    let state = &mut test_state(
        &block_context.chain_info,
        BALANCE,
        &[(account_contract, 1), (test_contract, 1)],
    );
    let sender_address = account_contract.get_instance_address(0);
    let default_invoke_args = invoke_tx_args! {
        sender_address,
        calldata: create_trivial_calldata(test_contract.get_instance_address(0)
    )};

    // Deploy.
    let invalid_tx = AccountTransaction::new(deploy_account_tx(
        deploy_account_tx_args! {
            resource_bounds,
            class_hash: test_contract.get_class_hash()
        },
        &mut NonceManager::default(),
    ));
    assert_resource_bounds_exceed_balance_failure(state, block_context, invalid_tx);

    // V1 Invoke.
    let invalid_max_fee = Fee(BALANCE.0 + 1);
    let invalid_tx = account_invoke_tx(invoke_tx_args! {
        max_fee: invalid_max_fee,
        version: TransactionVersion::ONE,
        ..default_invoke_args.clone()
    });
    assert_resource_bounds_exceed_balance_failure(state, block_context, invalid_tx);

    // V3 txs.
    macro_rules! assert_resource_overdraft {
        ($invalid_resource_bounds:expr) => {
            // V3 invoke.
            let invalid_tx = account_invoke_tx(invoke_tx_args! {
                resource_bounds: $invalid_resource_bounds,
                ..default_invoke_args.clone()
            });
            assert_resource_bounds_exceed_balance_failure(state, block_context, invalid_tx);
            // Declare.
            let contract_to_declare = FeatureContract::Empty(CairoVersion::Cairo1);
            let class_info = calculate_class_info_for_testing(contract_to_declare.get_class());
            let invalid_tx = AccountTransaction::new(declare_tx(
                declare_tx_args! {
                    class_hash: contract_to_declare.get_class_hash(),
                    compiled_class_hash: contract_to_declare.get_compiled_class_hash(),
                    sender_address,
                    resource_bounds: $invalid_resource_bounds,
                },
                class_info,
            ));
            assert_resource_bounds_exceed_balance_failure(state, block_context, invalid_tx);
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
                create_all_resource_bounds(
                    l1_gas_amount,
                    DEFAULT_STRK_L1_GAS_PRICE.into(),
                    l2_gas_amount,
                    DEFAULT_STRK_L2_GAS_PRICE.into(),
                    l1_data_gas_amount,
                    DEFAULT_STRK_L1_DATA_GAS_PRICE.into(),
                )
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
        }
    }
}

#[rstest]
fn test_insufficient_new_resource_bounds_pre_validation(
    mut block_context: BlockContext,
    #[values(true, false)] use_kzg_da: bool,
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1)] account_cairo_version: CairoVersion,
) {
    block_context.block_info.use_kzg_da = use_kzg_da;
    let block_context = &block_context;
    let account_contract = FeatureContract::AccountWithoutValidations(account_cairo_version);
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo0);
    let state = &mut test_state(
        &block_context.chain_info,
        BALANCE,
        &[(account_contract, 1), (test_contract, 1)],
    );
    let valid_invoke_tx_args = invoke_tx_args! {
        sender_address: account_contract.get_instance_address(0),
        calldata: create_trivial_calldata(test_contract.get_instance_address(0)),
        max_fee: MAX_FEE
    };
    let tx = &account_invoke_tx(valid_invoke_tx_args.clone());

    // V3 transaction.
    let GasPriceVector {
        l1_gas_price: actual_strk_l1_gas_price,
        l1_data_gas_price: actual_strk_l1_data_gas_price,
        l2_gas_price: actual_strk_l2_gas_price,
    } = block_context.block_info.gas_prices.strk_gas_prices;

    let minimal_gas_vector =
        estimate_minimal_gas_vector(block_context, tx, &GasVectorComputationMode::All);

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
    let valid_resources_tx = account_invoke_tx(InvokeTxArgs {
        resource_bounds: ValidResourceBounds::AllResources(default_resource_bounds),
        ..valid_invoke_tx_args.clone()
    })
    .execute(state, block_context);

    let next_nonce = match valid_resources_tx {
        Ok(_) => 1,
        Err(err) => match err {
            TransactionExecutionError::TransactionPreValidationError(
                TransactionPreValidationError::TransactionFeeError(
                    TransactionFeeError::MaxGasAmountTooLow { .. },
                ),
            ) => panic!("Transaction failed with expected minimal amount."),
            TransactionExecutionError::TransactionPreValidationError(
                TransactionPreValidationError::TransactionFeeError(
                    TransactionFeeError::MaxGasPriceTooLow { .. },
                ),
            ) => panic!("Transaction failed with expected minimal price."),
            // Ignore failures other than those above (e.g., post-validation errors).
            _ => 0,
        },
    };

    // Max gas amount too low, new resource bounds.
    // TODO(Aner): add a test for more than 1 insufficient resource amount, after error message
    // contains all insufficient resources.
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
        let invalid_v3_tx = account_invoke_tx(InvokeTxArgs {
            resource_bounds: ValidResourceBounds::AllResources(invalid_resources),
            nonce: nonce!(next_nonce),
            ..valid_invoke_tx_args.clone()
        });
        let execution_error = invalid_v3_tx.execute(state, block_context).unwrap_err();
        assert_matches!(
            execution_error,
            TransactionExecutionError::TransactionPreValidationError(
                TransactionPreValidationError::TransactionFeeError(
                    TransactionFeeError::MaxGasAmountTooLow{
                        resource,
                        ..}))
            if resource == insufficient_resource
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

        let invalid_v3_tx = account_invoke_tx(InvokeTxArgs {
            resource_bounds: ValidResourceBounds::AllResources(invalid_resources),
            nonce: nonce!(next_nonce),
            ..valid_invoke_tx_args.clone()
        });
        let execution_error = invalid_v3_tx.execute(state, block_context).unwrap_err();
        assert_matches!(
            execution_error,
            TransactionExecutionError::TransactionPreValidationError(
                TransactionPreValidationError::TransactionFeeError(
                    TransactionFeeError::MaxGasPriceTooLow{
                        resource,
                        ..}))
            if resource == insufficient_resource
        );
    }
}

#[rstest]
fn test_insufficient_deprecated_resource_bounds_pre_validation(
    block_context: BlockContext,
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1)] account_cairo_version: CairoVersion,
) {
    let block_context = &block_context;
    let account_contract = FeatureContract::AccountWithoutValidations(account_cairo_version);
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo0);
    let state = &mut test_state(
        &block_context.chain_info,
        BALANCE,
        &[(account_contract, 1), (test_contract, 1)],
    );
    let valid_invoke_tx_args = invoke_tx_args! {
        sender_address: account_contract.get_instance_address(0),
        calldata: create_trivial_calldata(test_contract.get_instance_address(0)),
        max_fee: MAX_FEE
    };

    // The minimal gas estimate does not depend on tx version.
    let tx = &account_invoke_tx(valid_invoke_tx_args.clone());
    let minimal_l1_gas =
        estimate_minimal_gas_vector(block_context, tx, &GasVectorComputationMode::NoL2Gas).l1_gas;

    // Test V1 transaction.

    let gas_prices = &block_context.block_info.gas_prices;
    // TODO(Aner, 21/01/24) change to linear combination.
    let minimal_fee =
        minimal_l1_gas.checked_mul(gas_prices.eth_gas_prices.l1_gas_price.get()).unwrap();
    // Max fee too low (lower than minimal estimated fee).
    let invalid_max_fee = Fee(minimal_fee.0 - 1);
    let invalid_v1_tx = account_invoke_tx(
        invoke_tx_args! { max_fee: invalid_max_fee, version: TransactionVersion::ONE,  ..valid_invoke_tx_args.clone() },
    );
    let execution_error = invalid_v1_tx.execute(state, block_context).unwrap_err();

    // Test error.
    assert_matches!(
        execution_error,
        TransactionExecutionError::TransactionPreValidationError(
            TransactionPreValidationError::TransactionFeeError(
                TransactionFeeError::MaxFeeTooLow {  min_fee, max_fee }))
        if max_fee == invalid_max_fee && min_fee == minimal_fee
    );

    // Test V3 transaction.
    let actual_strk_l1_gas_price = gas_prices.strk_gas_prices.l1_gas_price;

    // Max L1 gas amount too low, old resource bounds.
    // TODO(Ori, 1/2/2024): Write an indicative expect message explaining why the conversion works.
    let insufficient_max_l1_gas_amount = (minimal_l1_gas.0 - 1).into();
    let invalid_v3_tx = account_invoke_tx(invoke_tx_args! {
        resource_bounds: l1_resource_bounds(insufficient_max_l1_gas_amount, actual_strk_l1_gas_price.into()),
        ..valid_invoke_tx_args.clone()
    });
    let execution_error = invalid_v3_tx.execute(state, block_context).unwrap_err();
    assert_matches!(
        execution_error,
        TransactionExecutionError::TransactionPreValidationError(
            TransactionPreValidationError::TransactionFeeError(
                TransactionFeeError::MaxGasAmountTooLow{
                    resource,
                    max_gas_amount,
                    minimal_gas_amount}))
        if max_gas_amount == insufficient_max_l1_gas_amount &&
        minimal_gas_amount == minimal_l1_gas && resource == L1Gas
    );

    // Max L1 gas price too low, old resource bounds.
    let insufficient_max_l1_gas_price = (actual_strk_l1_gas_price.get().0 - 1).into();
    let invalid_v3_tx = account_invoke_tx(invoke_tx_args! {
        resource_bounds: l1_resource_bounds(minimal_l1_gas, insufficient_max_l1_gas_price),
        ..valid_invoke_tx_args.clone()
    });
    let execution_error = invalid_v3_tx.execute(state, block_context).unwrap_err();
    assert_matches!(
        execution_error,
        TransactionExecutionError::TransactionPreValidationError(
            TransactionPreValidationError::TransactionFeeError(
                TransactionFeeError::MaxGasPriceTooLow{ resource: L1Gas ,max_gas_price: max_l1_gas_price, actual_gas_price: actual_l1_gas_price }))
        if max_l1_gas_price == insufficient_max_l1_gas_price &&
        actual_l1_gas_price == actual_strk_l1_gas_price.into()
    );
}

#[rstest]
#[case::l1_bounds(default_l1_resource_bounds(), Resource::L1Gas)]
#[case::all_bounds_l1_gas_overdraft(default_all_resource_bounds(), Resource::L1Gas)]
#[case::all_bounds_l2_gas_overdraft(default_all_resource_bounds(), Resource::L2Gas)]
#[case::all_bounds_l1_data_gas_overdraft(default_all_resource_bounds(), Resource::L1DataGas)]
fn test_actual_fee_gt_resource_bounds(
    mut block_context: BlockContext,
    #[case] resource_bounds: ValidResourceBounds,
    #[case] overdraft_resource: Resource,
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1)] account_cairo_version: CairoVersion,
) {
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
    let tx = &account_invoke_tx(tx_args.clone());
    let execution_result = tx.execute(state, block_context).unwrap();
    let mut actual_gas = execution_result.receipt.gas;

    // Create new gas bounds that are lower than the actual gas.
    let (expected_fee, overdraft_resource_bounds) = match gas_mode {
        GasVectorComputationMode::NoL2Gas => {
            let l1_gas_bound = GasAmount(actual_gas.to_discounted_l1_gas(gas_prices).0 - 1);
            (
                GasVector::from_l1_gas(l1_gas_bound).cost(gas_prices),
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
                actual_gas.cost(gas_prices),
                ValidResourceBounds::all_bounds_from_vectors(&actual_gas, gas_prices),
            )
        }
    };
    let invalid_tx = account_invoke_tx(invoke_tx_args! {
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
fn test_invalid_nonce(
    block_context: BlockContext,
    default_all_resource_bounds: ValidResourceBounds,
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1)] account_cairo_version: CairoVersion,
) {
    let account_contract = FeatureContract::AccountWithoutValidations(account_cairo_version);
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo0);
    let state = &mut test_state(
        &block_context.chain_info,
        BALANCE,
        &[(account_contract, 1), (test_contract, 1)],
    );
    let valid_invoke_tx_args = invoke_tx_args! {
        sender_address: account_contract.get_instance_address(0),
        calldata: create_trivial_calldata(test_contract.get_instance_address(0)),
        resource_bounds: default_all_resource_bounds,
    };
    let mut transactional_state = TransactionalState::create_transactional(state);

    // Strict, negative flow: account nonce = 0, incoming tx nonce = 1.
    let invalid_nonce = nonce!(1_u8);
    let invalid_tx =
        account_invoke_tx(invoke_tx_args! { nonce: invalid_nonce, ..valid_invoke_tx_args.clone() });
    let invalid_tx_context = block_context.to_tx_context(&invalid_tx);
    let pre_validation_err = invalid_tx
        .perform_pre_validation_stage(&mut transactional_state, &invalid_tx_context, false, true)
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
    let valid_tx =
        account_invoke_tx(invoke_tx_args! { nonce: valid_nonce, ..valid_invoke_tx_args.clone() });

    let valid_tx_context = block_context.to_tx_context(&valid_tx);
    valid_tx
        .perform_pre_validation_stage(&mut transactional_state, &valid_tx_context, false, false)
        .unwrap();

    // Negative flow: account nonce = 1, incoming tx nonce = 0.
    let invalid_nonce = nonce!(0_u8);
    let invalid_tx =
        account_invoke_tx(invoke_tx_args! { nonce: invalid_nonce, ..valid_invoke_tx_args.clone() });
    let invalid_tx_context = block_context.to_tx_context(&invalid_tx);
    let pre_validation_err = invalid_tx
        .perform_pre_validation_stage(&mut transactional_state, &invalid_tx_context, false, false)
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
        expected_validate_call_info(
            account_class_hash,
            constants::VALIDATE_DECLARE_ENTRY_POINT_NAME,
            0,
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
    // TODO: Make TransactionVersion an enum and use match here.
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
#[case(TransactionVersion::ZERO, CairoVersion::Cairo0)]
#[case(TransactionVersion::ONE, CairoVersion::Cairo0)]
#[case(TransactionVersion::TWO, CairoVersion::Cairo1)]
#[case(TransactionVersion::THREE, CairoVersion::Cairo1)]
fn test_declare_tx(
    default_all_resource_bounds: ValidResourceBounds,
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1)] account_cairo_version: CairoVersion,
    #[case] tx_version: TransactionVersion,
    #[case] empty_contract_version: CairoVersion,
    #[values(false, true)] use_kzg_da: bool,
) {
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
    let account_tx = AccountTransaction::new(declare_tx(
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
        account.get_runnable_class().tracked_resource(
            &versioned_constants.min_compiler_version_for_sierra_gas,
            tx_context.tx_info.gas_mode(),
        ),
        if tx_version >= TransactionVersion::THREE {
            user_initial_gas_from_bounds(default_all_resource_bounds)
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
            FeatureContract::ERC20(CairoVersion::Cairo0).get_class_hash(),
        )
    };

    let da_gas = starknet_resources.state.da_gas_vector(use_kzg_da);
    let expected_cairo_resources = get_expected_cairo_resources(
        versioned_constants,
        TransactionType::Declare,
        &starknet_resources,
        vec![&expected_validate_call_info],
    );
    let mut expected_actual_resources = TransactionResources {
        starknet_resources,
        computation: ComputationResources {
            vm_resources: expected_cairo_resources,
            ..Default::default()
        },
    };

    add_kzg_da_resources_to_resources_mapping(
        &mut expected_actual_resources.computation.vm_resources,
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
    let account_tx2 = AccountTransaction::new(declare_tx(
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
fn test_declare_tx_v0(default_l1_resource_bounds: ValidResourceBounds) {
    let tx_version = TransactionVersion::ZERO;
    let block_context = &BlockContext::create_for_account_testing();
    let empty_contract = FeatureContract::Empty(CairoVersion::Cairo0);
    let account = FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1);
    let chain_info = &block_context.chain_info;
    let state = &mut test_state(chain_info, BALANCE, &[(account, 1)]);
    let class_hash = empty_contract.get_class_hash();
    let compiled_class_hash = empty_contract.get_compiled_class_hash();
    let class_info = calculate_class_info_for_testing(empty_contract.get_class());
    let sender_address = account.get_instance_address(0);
    let mut nonce_manager = NonceManager::default();

    let tx = declare_tx(
        declare_tx_args! {
            max_fee: Fee(0),
            sender_address,
            version: tx_version,
            resource_bounds: default_l1_resource_bounds,
            class_hash,
            compiled_class_hash,
            nonce: nonce_manager.next(sender_address),
        },
        class_info.clone(),
    );
    let account_tx = AccountTransaction {
        tx,
        execution_flags: ExecutionFlags { charge_fee: false, ..ExecutionFlags::default() },
    };

    let actual_execution_info = account_tx.execute(state, block_context).unwrap(); // fee not charged for declare v0.

    assert_eq!(actual_execution_info.fee_transfer_call_info, None, "not none");
    assert_eq!(actual_execution_info.receipt.fee, Fee(0));
}

#[rstest]
fn test_deploy_account_tx(
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1)] cairo_version: CairoVersion,
    #[values(false, true)] use_kzg_da: bool,
    default_all_resource_bounds: ValidResourceBounds,
) {
    let block_context = &BlockContext::create_for_account_testing_with_kzg(use_kzg_da);
    let versioned_constants = &block_context.versioned_constants;
    let chain_info = &block_context.chain_info;
    let mut nonce_manager = NonceManager::default();
    let account = FeatureContract::AccountWithoutValidations(cairo_version);
    let account_class_hash = account.get_class_hash();
    let state = &mut test_state(chain_info, BALANCE, &[(account, 1)]);
    let deploy_account = AccountTransaction::new(deploy_account_tx(
        deploy_account_tx_args! {
            resource_bounds: default_all_resource_bounds,
            class_hash: account_class_hash
        },
        &mut nonce_manager,
    ));

    // Extract deploy account transaction fields for testing, as it is consumed when creating an
    // account transaction.
    let class_hash = deploy_account.class_hash().unwrap();
    let deployed_account_address = deploy_account.sender_address();
    let user_initial_gas = user_initial_gas_from_bounds(default_all_resource_bounds);

    // Update the balance of the about to be deployed account contract in the erc20 contract, so it
    // can pay for the transaction execution.
    let deployed_account_balance_key = get_fee_token_var_address(deployed_account_address);
    for fee_type in FeeType::iter() {
        state
            .set_storage_at(
                chain_info.fee_token_address(&fee_type),
                deployed_account_balance_key,
                felt!(BALANCE.0),
            )
            .unwrap();
    }

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

    let expected_gas_consumed = 0;
    let expected_validate_call_info = expected_validate_call_info(
        account_class_hash,
        constants::VALIDATE_DEPLOY_ENTRY_POINT_NAME,
        expected_gas_consumed,
        validate_calldata,
        deployed_account_address,
        cairo_version,
        account.get_runnable_class().tracked_resource(
            &versioned_constants.min_compiler_version_for_sierra_gas,
            tx_context.tx_info.gas_mode(),
        ),
        user_initial_gas,
    );

    // Build expected execute call info.
    let expected_execute_call_info = Some(CallInfo {
        call: CallEntryPoint {
            class_hash: Some(account_class_hash),
            code_address: None,
            entry_point_type: EntryPointType::Constructor,
            entry_point_selector: selector_from_name(CONSTRUCTOR_ENTRY_POINT_NAME),
            storage_address: deployed_account_address,
            initial_gas: user_initial_gas.unwrap_or(GasAmount(default_initial_gas_cost())).0,
            ..Default::default()
        },
        ..Default::default()
    });

    // Build expected fee transfer call info.
    let expected_actual_fee = actual_execution_info.receipt.fee;
    let expected_fee_transfer_call_info = expected_fee_transfer_call_info(
        tx_context,
        deployed_account_address,
        expected_actual_fee,
        FeatureContract::ERC20(CairoVersion::Cairo0).get_class_hash(),
    );
    let starknet_resources = actual_execution_info.receipt.resources.starknet_resources.clone();

    let state_changes_count = StateChangesCount {
        n_storage_updates: 1,
        n_modified_contracts: 1,
        n_class_hash_updates: 1,
        ..StateChangesCount::default()
    };
    let da_gas = get_da_gas_cost(&state_changes_count, use_kzg_da);
    let expected_cairo_resources = get_expected_cairo_resources(
        &block_context.versioned_constants,
        TransactionType::DeployAccount,
        &starknet_resources,
        vec![&expected_validate_call_info, &expected_execute_call_info],
    );

    let mut actual_resources = TransactionResources {
        starknet_resources,
        computation: ComputationResources {
            vm_resources: expected_cairo_resources,
            ..Default::default()
        },
    };

    add_kzg_da_resources_to_resources_mapping(
        &mut actual_resources.computation.vm_resources,
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
    let deploy_account = AccountTransaction::new(deploy_account_tx(
        deploy_account_tx_args! {
            resource_bounds: default_all_resource_bounds,
            class_hash: account_class_hash
        },
        &mut nonce_manager,
    ));
    let error = deploy_account.execute(state, block_context).unwrap_err();
    assert_matches!(
        error,
        TransactionExecutionError::ContractConstructorExecutionFailed(
            ConstructorEntryPointExecutionError::ExecutionError {
                error: EntryPointExecutionError::StateError(
                    StateError::UnavailableContractAddress(_)
                ),
                ..
            }
        )
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
    let mut nonce_manager = NonceManager::default();
    let undeclared_hash = class_hash!("0xdeadbeef");
    let deploy_account = AccountTransaction::new(deploy_account_tx(
        deploy_account_tx_args! {
            resource_bounds: default_all_resource_bounds,  class_hash: undeclared_hash
        },
        &mut nonce_manager,
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
            ConstructorEntryPointExecutionError::ExecutionError {
                error: EntryPointExecutionError::StateError(
                    StateError::UndeclaredClassHash(class_hash)
                ),
                ..
            }
        )
        if class_hash == undeclared_hash
    );
}

// TODO(Arni, 1/5/2024): Cover other versions of declare transaction.
// TODO(Arni, 1/5/2024): Consider version 0 invoke.
#[rstest]
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
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1)] cairo_version: CairoVersion,
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
        charge_fee: false, // We test `__validate__`, and don't care about the cahrge fee flow.
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
    check_tx_execution_error_for_custom_hint!(
        &error,
        "Unauthorized syscall call_contract in execution mode Validate.",
        validate_constructor,
    );

    if let CairoVersion::Cairo1 = cairo_version {
        // Try to use the syscall get_block_hash (forbidden).
        let account_tx = create_account_tx_for_validate_test_nonce_0(FaultyAccountTxCreatorArgs {
            scenario: GET_BLOCK_HASH,
            contract_address_salt: salt_manager.next_salt(),
            additional_data: None,
            resource_bounds: ValidResourceBounds::create_for_testing_no_fee_enforcement(),
            ..default_args
        });
        let error = account_tx.execute(state, block_context).unwrap_err();
        check_tx_execution_error_for_custom_hint!(
            &error,
            "Unauthorized syscall get_block_hash in execution mode Validate.",
            validate_constructor,
        );
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

    if let CairoVersion::Cairo1 = cairo_version {
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

#[rstest]
fn test_valid_flag(
    block_context: BlockContext,
    default_all_resource_bounds: ValidResourceBounds,
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1)] account_cairo_version: CairoVersion,
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1)] test_contract_cairo_version: CairoVersion,
) {
    let block_context = &block_context;
    let account_contract = FeatureContract::AccountWithoutValidations(account_cairo_version);
    let test_contract = FeatureContract::TestContract(test_contract_cairo_version);
    let state = &mut test_state(
        &block_context.chain_info,
        BALANCE,
        &[(account_contract, 1), (test_contract, 1)],
    );

    let tx = invoke_tx(invoke_tx_args! {
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
    let account_balance = BALANCE;
    let block_context = &block_context;
    let account = FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1);
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1);
    let state = &mut test_state(
        &block_context.chain_info,
        account_balance,
        &[(account, 1), (test_contract, 1)],
    );
    let mut version = Felt::from(3_u8);
    if only_query {
        version += *QUERY_VERSION_BASE;
    }
    let sender_address = account.get_instance_address(0);
    let test_contract_address = test_contract.get_instance_address(0);
    let expected_tx_info = vec![
        version,                                         // Transaction version.
        *sender_address.0.key(),                         // Account address.
        Felt::ZERO,                                      // Max fee.
        Felt::ZERO,                                      // Signature.
        Felt::ZERO,                                      // Transaction hash.
        felt!(&*ChainId::create_for_testing().as_hex()), // Chain ID.
        Felt::ZERO,                                      // Nonce.
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
        *sender_address.0.key(),        // Caller address.
        *test_contract_address.0.key(), // Storage address.
        entry_point_selector.0,         // Entry point selector.
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
        *test_contract_address.0.key(), // Contract address.
        entry_point_selector.0,         // EP selector.
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
    let tx = crate::test_utils::invoke::invoke_tx(invoke_tx_args! {
        calldata: execute_calldata,
        resource_bounds: default_all_resource_bounds,
        sender_address,
    });
    let execution_flags = ExecutionFlags { only_query, ..Default::default() };
    let invoke_tx = AccountTransaction { tx, execution_flags };

    let tx_execution_info = invoke_tx.execute(state, block_context).unwrap();
    assert_eq!(tx_execution_info.revert_error, None);
}

#[rstest]
fn test_l1_handler(#[values(false, true)] use_kzg_da: bool) {
    let gas_mode = GasVectorComputationMode::NoL2Gas;
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1);
    let chain_info = &ChainInfo::create_for_testing();
    let state = &mut test_state(chain_info, BALANCE, &[(test_contract, 1)]);
    let block_context = &BlockContext::create_for_account_testing_with_kzg(use_kzg_da);
    let contract_address = test_contract.get_instance_address(0);
    let versioned_constants = &block_context.versioned_constants;
    let tx = l1handler_tx(Fee(1), contract_address);
    let calldata = tx.tx.calldata.clone();
    let key = calldata.0[1];
    let value = calldata.0[2];
    let payload_size = tx.payload_size();
    let actual_execution_info = tx.execute(state, block_context).unwrap();

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
            initial_gas: default_initial_gas_cost(),
        },
        execution: CallExecution {
            retdata: Retdata(vec![value]),
            gas_consumed: 6120,
            ..Default::default()
        },
        charged_resources: ChargedResources::from_execution_resources(ExecutionResources {
            n_steps: 151,
            n_memory_holes: 0,
            builtin_instance_counter: HashMap::from([(BuiltinName::range_check, 6)]),
        }),
        accessed_storage_keys: HashSet::from_iter(vec![accessed_storage_key]),
        tracked_resource: test_contract.get_runnable_class().tracked_resource(
            &versioned_constants.min_compiler_version_for_sierra_gas,
            GasVectorComputationMode::NoL2Gas,
        ),
        ..Default::default()
    };

    // Build the expected resource mapping.
    // TODO(Nimrod, 1/5/2024): Change these hard coded values to match to the transaction resources
    // (currently matches only starknet resources).
    let expected_gas = match use_kzg_da {
        true => GasVector {
            l1_gas: 17988_u32.into(),
            l1_data_gas: 160_u32.into(),
            l2_gas: 0_u32.into(),
        },
        false => GasVector::from_l1_gas(19682_u32.into()),
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

    let mut expected_execution_resources = ExecutionResources {
        builtin_instance_counter: HashMap::from([
            (BuiltinName::pedersen, 11 + payload_size),
            (
                BuiltinName::range_check,
                get_tx_resources(TransactionType::L1Handler).builtin_instance_counter
                    [&BuiltinName::range_check]
                    + 6,
            ),
        ]),
        n_steps: get_tx_resources(TransactionType::L1Handler).n_steps + 164,
        n_memory_holes: 0,
    };

    add_kzg_da_resources_to_resources_mapping(
        &mut expected_execution_resources,
        &state_changes_count,
        versioned_constants,
        use_kzg_da,
    );

    // Copy StarknetResources from actual resources and assert gas usage calculation is correct.
    let expected_tx_resources = TransactionResources {
        starknet_resources: actual_execution_info.receipt.resources.starknet_resources.clone(),
        computation: ComputationResources {
            vm_resources: expected_execution_resources,
            ..Default::default()
        },
    };

    assert_eq!(actual_execution_info.receipt.resources, expected_tx_resources);
    assert_eq!(
        expected_gas,
        actual_execution_info.receipt.resources.to_gas_vector(
            versioned_constants,
            use_kzg_da,
            &gas_mode,
        )
    );

    let total_gas = expected_tx_resources.to_gas_vector(
        versioned_constants,
        block_context.block_info.use_kzg_da,
        &gas_mode,
    );

    // Build the expected execution info.
    let expected_execution_info = TransactionExecutionInfo {
        validate_call_info: None,
        execute_call_info: Some(expected_call_info),
        fee_transfer_call_info: None,
        receipt: TransactionReceipt {
            fee: Fee(0),
            da_gas: expected_da_gas,
            resources: expected_tx_resources,
            gas: total_gas,
        },
        revert_error: None,
    };

    // Check the actual returned execution info.
    assert_eq!(actual_execution_info, expected_execution_info);

    // Check the state changes.
    assert_eq!(
        state.get_storage_at(contract_address, StorageKey::try_from(key).unwrap(),).unwrap(),
        value,
    );
    // Negative flow: not enough fee paid on L1.

    // set the storage back to 0, so the fee will also include the storage write.
    // TODO(Meshi, 15/6/2024): change the l1_handler_set_value cairo function to
    // always uptade the storage instad.
    state.set_storage_at(contract_address, StorageKey::try_from(key).unwrap(), Felt::ZERO).unwrap();
    let tx_no_fee = l1handler_tx(Fee(0), contract_address);
    let error = tx_no_fee.execute(state, block_context).unwrap_err(); // Do not charge fee as L1Handler's resource bounds (/max fee) is 0.
    // Today, we check that the paid_fee is positive, no matter what was the actual fee.
    let expected_actual_fee =
        get_fee_by_gas_vector(&block_context.block_info, total_gas, &FeeType::Eth);

    assert_matches!(
        error,
        TransactionExecutionError::TransactionFeeError(
            TransactionFeeError::InsufficientFee { paid_fee, actual_fee }
        )
        if paid_fee == Fee(0) && actual_fee == expected_actual_fee
    );
}

#[rstest]
fn test_execute_tx_with_invalid_tx_version(
    block_context: BlockContext,
    default_all_resource_bounds: ValidResourceBounds,
) {
    let cairo_version = CairoVersion::Cairo0;
    let account = FeatureContract::AccountWithoutValidations(cairo_version);
    let test_contract = FeatureContract::TestContract(cairo_version);
    let block_context = &block_context;
    let state =
        &mut test_state(&block_context.chain_info, BALANCE, &[(account, 1), (test_contract, 1)]);
    let invalid_version = 12345_u64;
    let calldata = create_calldata(
        test_contract.get_instance_address(0),
        "test_tx_version",
        &[felt!(invalid_version)],
    );
    let account_tx = account_invoke_tx(invoke_tx_args! {
        resource_bounds: default_all_resource_bounds,
        sender_address: account.get_instance_address(0),
        calldata,
    });

    let execution_info = account_tx.execute(state, block_context).unwrap();
    assert!(
        execution_info
            .revert_error
            .unwrap()
            .to_string()
            .contains(format!("ASSERT_EQ instruction failed: {} != 3.", invalid_version).as_str())
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

#[rstest]
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
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1)] cairo_version: CairoVersion,
) {
    let test_contract = FeatureContract::TestContract(cairo_version);
    let account_contract = FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1);
    let block_context = &block_context;
    let state = &mut test_state(
        &block_context.chain_info,
        BALANCE,
        &[(test_contract, 1), (account_contract, 1)],
    );

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
            vec![test_contract.get_instance_address(0).into()],
            vec![selector_from_name("test_emit_events").0],
            vec![felt!(u16::try_from(calldata.len()).expect("Failed to convert usize to u16."))],
            calldata.clone(),
        ]
        .concat()
        .into(),
    );

    let account_tx = account_invoke_tx(invoke_tx_args! {
        sender_address: account_contract.get_instance_address(0),
        calldata: execute_calldata,
        resource_bounds: default_all_resource_bounds,
        nonce: nonce!(0_u8),
    });
    let execution_info = account_tx.execute(state, block_context).unwrap();
    match &expected_error {
        Some(expected_error) => {
            let error_string = execution_info.revert_error.unwrap().to_string();
            assert!(error_string.contains(&format!("{}", expected_error)));
        }
        None => {
            assert!(!execution_info.is_reverted());
        }
    }
}

#[test]
fn test_balance_print() {
    let int = balance_to_big_uint(&Felt::from(16_u64), &Felt::from(1_u64));
    assert!(format!("{}", int) == (BigUint::from(u128::MAX) + BigUint::from(17_u128)).to_string());
}
