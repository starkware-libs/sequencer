use std::collections::HashMap;

use blockifier::execution::call_info::{
    CallExecution,
    CallInfo,
    MessageToL1 as BlockifierMessageToL1,
    OrderedEvent,
    OrderedL2ToL1Message,
    Retdata,
};
use blockifier::execution::entry_point::{CallEntryPoint, CallType};
use blockifier::execution::stack_trace::{
    Cairo1RevertHeader,
    Cairo1RevertSummary,
    ErrorStack,
    ErrorStackHeader,
    ErrorStackSegment,
};
use blockifier::fee::receipt::TransactionReceipt;
use blockifier::fee::resources::{ComputationResources, StarknetResources, TransactionResources};
use blockifier::transaction::objects::{RevertError, TransactionExecutionInfo};
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use indexmap::indexmap;
use starknet_api::block_hash::block_hash_calculator::{
    TransactionHashingData,
    TransactionOutputForHash,
};
use starknet_api::contract_class::EntryPointType;
use starknet_api::core::{
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    EntryPointSelector,
    EthAddress,
    Nonce,
    PatriciaKey,
};
use starknet_api::execution_resources::{GasAmount, GasVector};
use starknet_api::state::{StorageKey, ThinStateDiff};
use starknet_api::transaction::fields::{Calldata, Fee, TransactionSignature};
use starknet_api::transaction::{
    Event,
    EventContent,
    EventData,
    EventKey,
    L2ToL1Payload,
    MessageToL1 as StarknetApiMessageToL1,
    RevertedTransactionExecutionStatus,
    TransactionExecutionStatus,
    TransactionHash,
};
use starknet_types_core::felt::Felt;

pub(crate) fn get_transaction_output_for_hash(
    execution_status: &TransactionExecutionStatus,
) -> TransactionOutputForHash {
    let expected_execution_status = match execution_status {
        TransactionExecutionStatus::Succeeded => TransactionExecutionStatus::Succeeded,
        TransactionExecutionStatus::Reverted(_) => {
            TransactionExecutionStatus::Reverted(RevertedTransactionExecutionStatus {
                revert_reason: "reason".to_owned(),
            })
        }
    };
    TransactionOutputForHash {
        actual_fee: Fee(0),
        events: vec![Event {
            from_address: ContractAddress(PatriciaKey::from(2_u128)),
            content: EventContent {
                keys: vec![EventKey(1.into())],
                data: EventData(vec![0.into()]),
            },
        }],
        execution_status: expected_execution_status,
        gas_consumed: GasVector {
            l1_gas: GasAmount(0),
            l2_gas: GasAmount(0),
            l1_data_gas: GasAmount(64),
        },
        messages_sent: vec![StarknetApiMessageToL1 {
            from_address: ContractAddress(PatriciaKey::from(2_u128)),
            to_address: EthAddress::try_from(Felt::from(1)).expect("to_address"),
            payload: L2ToL1Payload(vec![0.into()]),
        }],
    }
}

pub(crate) fn get_thin_state_diff() -> ThinStateDiff {
    ThinStateDiff {
        deployed_contracts: indexmap! {
            ContractAddress::from(1_u128) => ClassHash(2.into())
        },
        storage_diffs: indexmap! {
            ContractAddress::from(7_u128) => indexmap! {
                StorageKey::from(8_u128) => 9.into(),
            },
        },
        class_hash_to_compiled_class_hash: indexmap! {
            ClassHash(13.into()) =>
                CompiledClassHash(14.into())
        },
        deprecated_declared_classes: vec![ClassHash(16.into()), ClassHash(15.into())],
        nonces: indexmap! {
            ContractAddress::from(3_u128) => Nonce(4.into()),
        },
    }
}

pub(crate) fn get_tx_data(execution_status: &TransactionExecutionStatus) -> TransactionHashingData {
    TransactionHashingData {
        transaction_signature: TransactionSignature(vec![1.into(), 2.into()].into()),
        transaction_output: get_transaction_output_for_hash(execution_status),
        transaction_hash: TransactionHash(3.into()),
    }
}

// Helper function to create complex ExecutionResources
fn create_execution_resources(
    steps: usize,
    memory_holes: usize,
    range_check_builtin: usize,
    pedersen_builtin: usize,
) -> ExecutionResources {
    ExecutionResources {
        n_steps: steps,
        n_memory_holes: memory_holes,
        builtin_instance_counter: HashMap::from([
            (cairo_vm::types::builtin_name::BuiltinName::range_check, range_check_builtin),
            (cairo_vm::types::builtin_name::BuiltinName::pedersen, pedersen_builtin),
            (cairo_vm::types::builtin_name::BuiltinName::bitwise, 1),
            (cairo_vm::types::builtin_name::BuiltinName::ec_op, 2),
        ]),
    }
}

// Helper function to create a CallEntryPoint
fn create_call_entry_point(
    contract_address: ContractAddress,
    entry_point_selector: EntryPointSelector,
    calldata: Vec<Felt>,
) -> CallEntryPoint {
    CallEntryPoint {
        class_hash: Some(ClassHash(5.into())),
        code_address: Some(contract_address),
        entry_point_type: EntryPointType::External,
        entry_point_selector,
        calldata: Calldata(calldata.into()),
        storage_address: contract_address,
        caller_address: ContractAddress::from(0_u128),
        call_type: CallType::Call,
        initial_gas: 1000000,
    }
}

// Helper function to create CallExecution
fn create_call_execution(
    retdata: Vec<Felt>,
    events: Vec<Event>,
    messages: Vec<StarknetApiMessageToL1>,
    failed: bool,
    gas_consumed: u64,
    events_counter: &mut usize,
    messages_counter: &mut usize,
) -> CallExecution {
    let ordered_events: Vec<OrderedEvent> = events
        .into_iter()
        .map(|event| {
            let ordered_event = OrderedEvent { order: *events_counter, event: event.content };
            *events_counter += 1;
            ordered_event
        })
        .collect();

    let ordered_messages: Vec<OrderedL2ToL1Message> = messages
        .into_iter()
        .map(|message| {
            let ordered_message = OrderedL2ToL1Message {
                order: *messages_counter,
                message: BlockifierMessageToL1 {
                    to_address: message.to_address.into(),
                    payload: message.payload,
                },
            };
            *messages_counter += 1;
            ordered_message
        })
        .collect();

    CallExecution {
        retdata: Retdata(retdata),
        events: ordered_events,
        l2_to_l1_messages: ordered_messages,
        cairo_native: false,
        failed,
        gas_consumed,
    }
}

// Helper function to create complex CallInfo
fn create_call_info(
    call: CallEntryPoint,
    execution: CallExecution,
    inner_calls: Vec<CallInfo>,
) -> CallInfo {
    CallInfo {
        call,
        execution,
        inner_calls,
        resources: create_execution_resources(1000, 0, 10, 5),
        tracked_resource: blockifier::execution::contract_class::TrackedResource::CairoSteps,
        storage_access_tracker: Default::default(),
        builtin_counters: HashMap::from([
            (cairo_vm::types::builtin_name::BuiltinName::range_check, 10),
            (cairo_vm::types::builtin_name::BuiltinName::pedersen, 5),
        ]),
    }
}

// Helper function to create TransactionResources
fn create_transaction_resources() -> TransactionResources {
    TransactionResources {
        starknet_resources: StarknetResources::default(),
        computation: ComputationResources {
            tx_vm_resources: create_execution_resources(2000, 5, 20, 10),
            os_vm_resources: create_execution_resources(500, 1, 5, 2),
            n_reverted_steps: 0,
            sierra_gas: GasAmount(1000),
            reverted_sierra_gas: GasAmount(0),
        },
    }
}

pub(crate) fn get_tx_execution_infos() -> Vec<TransactionExecutionInfo> {
    let mut result = Vec::new();

    // Example 1: Successful transaction with validate, execute, and fee transfer calls
    {
        let mut events_counter = 0;
        let mut messages_counter = 0;

        let tx_execution_info = TransactionExecutionInfo {
            validate_call_info: Some(create_call_info(
                create_call_entry_point(
                    ContractAddress::from(100_u128),
                    EntryPointSelector(1.into()),
                    vec![42.into()],
                ),
                create_call_execution(
                    vec![Felt::ONE], // validation success
                    vec![],
                    vec![],
                    false,
                    50000,
                    &mut events_counter,
                    &mut messages_counter,
                ),
                vec![], // no inner calls for validation
            )),
            execute_call_info: Some(create_call_info(
                create_call_entry_point(
                    ContractAddress::from(200_u128),
                    EntryPointSelector(2.into()),
                    vec![123.into(), 200.into()],
                ),
                create_call_execution(
                    vec![201.into(), 202.into()],
                    vec![Event {
                        from_address: ContractAddress::from(200_u128),
                        content: EventContent {
                            keys: vec![EventKey(11.into()), EventKey(12.into())],
                            data: EventData(vec![21.into(), 22.into(), 23.into()]),
                        },
                    }],
                    vec![StarknetApiMessageToL1 {
                        from_address: ContractAddress::from(200_u128),
                        to_address: EthAddress::try_from(Felt::from(31))
                            .expect("valid eth address"),
                        payload: L2ToL1Payload(vec![41.into(), 42.into()]),
                    }],
                    false,
                    150000,
                    &mut events_counter,
                    &mut messages_counter,
                ),
                // Inner calls for complex execution
                vec![create_call_info(
                    create_call_entry_point(
                        ContractAddress::from(300_u128),
                        EntryPointSelector(3.into()),
                        vec![77.into()],
                    ),
                    create_call_execution(
                        vec![88.into()],
                        vec![],
                        vec![],
                        false,
                        25000,
                        &mut events_counter,
                        &mut messages_counter,
                    ),
                    vec![], // no nested inner calls
                )],
            )),
            fee_transfer_call_info: Some(create_call_info(
                create_call_entry_point(
                    ContractAddress::from(1_u128), // fee token contract
                    EntryPointSelector(99.into()), // transfer selector
                    vec![
                        100.into(), // recipient
                        250.into(), // amount low
                        Felt::ZERO, // amount high
                    ],
                ),
                create_call_execution(
                    vec![Felt::ONE], // transfer success
                    vec![],
                    vec![],
                    false,
                    30000,
                    &mut events_counter,
                    &mut messages_counter,
                ),
                vec![],
            )),
            revert_error: None,
            receipt: TransactionReceipt {
                fee: Fee(1000),
                gas: GasVector {
                    l1_gas: GasAmount(200000),
                    l2_gas: GasAmount(50000),
                    l1_data_gas: GasAmount(10000),
                },
                da_gas: GasVector {
                    l1_gas: GasAmount(5000),
                    l2_gas: GasAmount(1000),
                    l1_data_gas: GasAmount(10000),
                },
                resources: create_transaction_resources(),
            },
        };

        result.push(tx_execution_info);
    }
    // Example 2: Reverted transaction with error
    {
        let mut events_counter = 0;
        let mut messages_counter = 0;

        let tx_execution_info = TransactionExecutionInfo {
            validate_call_info: Some(create_call_info(
                create_call_entry_point(
                    ContractAddress::from(400_u128),
                    EntryPointSelector(4.into()),
                    vec![55.into()],
                ),
                create_call_execution(
                    vec![Felt::ONE],
                    vec![],
                    vec![],
                    false,
                    40000,
                    &mut events_counter,
                    &mut messages_counter,
                ),
                vec![],
            )),
            execute_call_info: None, // execution failed, so no execute call info
            fee_transfer_call_info: Some(create_call_info(
                create_call_entry_point(
                    ContractAddress::from(1_u128),
                    EntryPointSelector(99.into()),
                    vec![150.into(), 200.into(), Felt::ZERO],
                ),
                create_call_execution(
                    vec![Felt::ONE],
                    vec![],
                    vec![],
                    false,
                    25000,
                    &mut events_counter,
                    &mut messages_counter,
                ),
                vec![],
            )),
            revert_error: Some(RevertError::Execution(ErrorStack {
                header: ErrorStackHeader::Execution,
                stack: vec![ErrorStackSegment::Cairo1RevertSummary(Cairo1RevertSummary {
                    header: Cairo1RevertHeader::Execution,
                    stack: vec![],
                    last_retdata: Retdata(vec![66.into()]),
                })],
            })),
            receipt: TransactionReceipt {
                fee: Fee(500), // reduced fee for failed transaction
                gas: GasVector {
                    l1_gas: GasAmount(100000),
                    l2_gas: GasAmount(25000),
                    l1_data_gas: GasAmount(5000),
                },
                da_gas: GasVector {
                    l1_gas: GasAmount(2500),
                    l2_gas: GasAmount(500),
                    l1_data_gas: GasAmount(5000),
                },
                resources: create_transaction_resources(),
            },
        };

        result.push(tx_execution_info);
    }
    // Example 3: L1 Handler transaction (no validation or fee transfer)
    {
        let mut events_counter = 0;
        let mut messages_counter = 0;

        let tx_execution_info = TransactionExecutionInfo {
            validate_call_info: None,
            execute_call_info: Some(create_call_info(
                create_call_entry_point(
                    ContractAddress::from(500_u128),
                    EntryPointSelector(7.into()),
                    vec![77.into(), 88.into(), 99.into()],
                ),
                create_call_execution(
                    vec![],
                    vec![Event {
                        from_address: ContractAddress::from(500_u128),
                        content: EventContent {
                            keys: vec![EventKey(51.into())],
                            data: EventData(vec![61.into(), 62.into()]),
                        },
                    }],
                    vec![],
                    false,
                    80000,
                    &mut events_counter,
                    &mut messages_counter,
                ),
                vec![],
            )),
            fee_transfer_call_info: None,
            revert_error: None,
            receipt: TransactionReceipt {
                fee: Fee(0), // L1 handler has no fee
                gas: GasVector {
                    l1_gas: GasAmount(80000),
                    l2_gas: GasAmount(0),
                    l1_data_gas: GasAmount(0),
                },
                da_gas: GasVector::default(),
                resources: create_transaction_resources(),
            },
        };

        result.push(tx_execution_info);
    }
    // Example 4: Transaction with multiple events and messages
    {
        let mut events_counter = 0;
        let mut messages_counter = 0;

        let tx_execution_info = TransactionExecutionInfo {
            validate_call_info: Some(create_call_info(
                create_call_entry_point(
                    ContractAddress::from(600_u128),
                    EntryPointSelector(8.into()),
                    vec![100.into()],
                ),
                create_call_execution(
                    vec![Felt::ONE],
                    vec![],
                    vec![],
                    false,
                    30000,
                    &mut events_counter,
                    &mut messages_counter,
                ),
                vec![],
            )),
            execute_call_info: Some(create_call_info(
                create_call_entry_point(
                    ContractAddress::from(600_u128),
                    EntryPointSelector(9.into()),
                    vec![150.into(), 250.into()],
                ),
                create_call_execution(
                    vec![Felt::ONE, 123.into()],
                    vec![
                        Event {
                            from_address: ContractAddress::from(600_u128),
                            content: EventContent {
                                keys: vec![EventKey(100.into()), EventKey(101.into())],
                                data: EventData(vec![200.into(), 300.into(), 400.into()]),
                            },
                        },
                        Event {
                            from_address: ContractAddress::from(600_u128),
                            content: EventContent {
                                keys: vec![EventKey(102.into())],
                                data: EventData(vec![500.into()]),
                            },
                        },
                        Event {
                            from_address: ContractAddress::from(600_u128),
                            content: EventContent {
                                keys: vec![
                                    EventKey(103.into()),
                                    EventKey(104.into()),
                                    EventKey(105.into()),
                                ],
                                data: EventData(vec![600.into(), 700.into()]),
                            },
                        },
                    ],
                    vec![
                        StarknetApiMessageToL1 {
                            from_address: ContractAddress::from(600_u128),
                            to_address: EthAddress::try_from(Felt::from(50))
                                .expect("valid eth address"),
                            payload: L2ToL1Payload(vec![80.into(), 90.into()]),
                        },
                        StarknetApiMessageToL1 {
                            from_address: ContractAddress::from(600_u128),
                            to_address: EthAddress::try_from(Felt::from(51))
                                .expect("valid eth address"),
                            payload: L2ToL1Payload(vec![100.into(), 110.into(), 120.into()]),
                        },
                    ],
                    false,
                    200000,
                    &mut events_counter,
                    &mut messages_counter,
                ),
                vec![], // no inner calls for this example
            )),
            fee_transfer_call_info: Some(create_call_info(
                create_call_entry_point(
                    ContractAddress::from(1_u128),
                    EntryPointSelector(99.into()),
                    vec![180.into(), 190.into(), Felt::ZERO],
                ),
                create_call_execution(
                    vec![Felt::ONE],
                    vec![],
                    vec![],
                    false,
                    35000,
                    &mut events_counter,
                    &mut messages_counter,
                ),
                vec![],
            )),
            revert_error: None,
            receipt: TransactionReceipt {
                fee: Fee(1500),
                gas: GasVector {
                    l1_gas: GasAmount(250000),
                    l2_gas: GasAmount(60000),
                    l1_data_gas: GasAmount(12000),
                },
                da_gas: GasVector {
                    l1_gas: GasAmount(6000),
                    l2_gas: GasAmount(1200),
                    l1_data_gas: GasAmount(12000),
                },
                resources: create_transaction_resources(),
            },
        };

        result.push(tx_execution_info);
    }

    result
}
