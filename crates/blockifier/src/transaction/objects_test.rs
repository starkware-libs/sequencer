use cairo_vm::types::builtin_name::BuiltinName;
use rstest::rstest;
use starknet_api::core::{ClassHash, ContractAddress, EthAddress};
use starknet_api::execution_resources::GasAmount;
use starknet_api::state::StorageKey;
use starknet_api::transaction::L2ToL1Payload;
use starknet_api::{class_hash, contract_address, storage_key};
use starknet_types_core::felt::Felt;

use crate::blockifier_versioned_constants::VersionedConstants;
use crate::execution::call_info::{
    BuiltinCounterMap,
    CallExecution,
    CallInfo,
    ChargedResources,
    EventSummary,
    ExecutionSummary,
    MessageToL1,
    NumberOfCalls,
    OrderedEvent,
    OrderedL2ToL1Message,
    StorageAccessTracker,
};
use crate::execution::entry_point::CallEntryPoint;
use crate::transaction::objects::TransactionExecutionInfo;

#[derive(Debug, Default)]
pub struct TestExecutionSummary {
    pub gas_consumed: GasAmount,
    pub num_of_events: usize,
    pub num_of_messages: usize,
    pub class_hash: ClassHash,
    pub storage_address: ContractAddress,
    pub storage_key: StorageKey,
    pub builtin_counters: BuiltinCounterMap,
    pub inner_builtin_counters: BuiltinCounterMap,
}

impl TestExecutionSummary {
    pub fn new(
        gas_consumed: u64,
        num_of_events: usize,
        num_of_messages: usize,
        class_hash: ClassHash,
        storage_address: &str,
        storage_key: &str,
    ) -> Self {
        TestExecutionSummary {
            gas_consumed: GasAmount(gas_consumed),
            num_of_events,
            num_of_messages,
            class_hash,
            storage_address: contract_address!(storage_address),
            storage_key: storage_key!(storage_key),
            builtin_counters: BuiltinCounterMap::new(),
            inner_builtin_counters: BuiltinCounterMap::new(),
        }
    }

    pub fn update_builtin_counters(&mut self, builtin_counters: &BuiltinCounterMap) {
        self.builtin_counters.extend(builtin_counters);
    }

    pub fn update_inner_builtin_counters(&mut self, inner_builtin_counters: &BuiltinCounterMap) {
        self.inner_builtin_counters.extend(inner_builtin_counters);
    }

    pub fn to_call_info(&self) -> CallInfo {
        CallInfo {
            call: CallEntryPoint {
                class_hash: Some(self.class_hash),
                storage_address: self.storage_address,
                ..Default::default()
            },
            execution: CallExecution {
                events: (0..self.num_of_events).map(|_| OrderedEvent::default()).collect(),
                l2_to_l1_messages: (0..self.num_of_messages)
                    .map(|i| OrderedL2ToL1Message {
                        order: i,
                        message: MessageToL1 {
                            to_address: EthAddress::default(),
                            payload: L2ToL1Payload(vec![Felt::default()]),
                        },
                    })
                    .collect(),
                gas_consumed: self.gas_consumed.0,
                ..Default::default()
            },
            storage_access_tracker: StorageAccessTracker {
                accessed_storage_keys: vec![self.storage_key].into_iter().collect(),
                ..Default::default()
            },
            builtin_counters: self.builtin_counters.clone(),
            inner_calls: vec![inner_call_info(&self.inner_builtin_counters)],
            ..Default::default()
        }
    }
}

fn shared_call_info() -> CallInfo {
    CallInfo {
        call: CallEntryPoint { class_hash: Some(class_hash!("0x1")), ..Default::default() },
        ..Default::default()
    }
}

fn inner_call_info(builtin_counters: &BuiltinCounterMap) -> CallInfo {
    CallInfo {
        call: CallEntryPoint { class_hash: Some(class_hash!("0x1")), ..Default::default() },
        builtin_counters: builtin_counters.clone(),
        ..Default::default()
    }
}

fn call_info_with_x_events(n_events: usize, n_inner_calls: usize) -> CallInfo {
    CallInfo {
        execution: CallExecution {
            events: (0..n_events).map(|_| OrderedEvent::default()).collect(),
            ..Default::default()
        },
        inner_calls: (0..n_inner_calls).map(|_| call_info_with_x_events(1, 0)).collect(),
        ..shared_call_info()
    }
}

fn call_info_with_deep_inner_calls(
    n_events: usize,
    n_inner_calls: usize,
    n_events_of_each_inner_call: usize,
    n_inner_calls_of_each_inner_call: usize,
) -> CallInfo {
    let inner_calls = (0..n_inner_calls)
        .map(|_| {
            call_info_with_x_events(n_events_of_each_inner_call, n_inner_calls_of_each_inner_call)
        })
        .collect();

    CallInfo {
        inner_calls,
        execution: CallExecution {
            events: (0..n_events).map(|_| OrderedEvent::default()).collect(),
            ..Default::default()
        },
        ..shared_call_info()
    }
}

#[rstest]
#[case(0, 0)]
#[case(0, 2)]
#[case(1, 3)]
#[case(2, 0)]
fn test_events_counter_in_tx_execution_info(
    #[case] n_execute_events: usize,
    #[case] n_inner_calls: usize,
) {
    let n_validate_events = 2;
    let n_fee_transfer_events = 1;

    let tx_execution_info = TransactionExecutionInfo {
        validate_call_info: Some(call_info_with_x_events(n_validate_events, 0)),
        execute_call_info: Some(call_info_with_x_events(n_execute_events, n_inner_calls)),
        fee_transfer_call_info: Some(call_info_with_x_events(n_fee_transfer_events, 0)),
        ..Default::default()
    };

    assert_eq!(
        tx_execution_info.summarize(VersionedConstants::latest_constants()).event_summary.n_events,
        n_validate_events + n_execute_events + n_fee_transfer_events + n_inner_calls
    );
}

#[rstest]
#[case(0)]
#[case(1)]
#[case(20)]
fn test_events_counter_in_tx_execution_info_with_inner_call_info(#[case] n_execute_events: usize) {
    let n_fee_transfer_events = 2;
    let n_inner_calls = 3;
    let n_execution_events = 1;
    let n_events_for_each_inner_call = 2;
    let n_inner_calls_of_each_inner_call = 1;

    let tx_execution_info = TransactionExecutionInfo {
        validate_call_info: Some(call_info_with_deep_inner_calls(
            n_execution_events,
            n_inner_calls,
            n_events_for_each_inner_call,
            n_inner_calls_of_each_inner_call,
        )),
        execute_call_info: Some(call_info_with_x_events(n_execute_events, 0)),
        fee_transfer_call_info: Some(call_info_with_x_events(n_fee_transfer_events, 0)),
        ..Default::default()
    };

    assert_eq!(
        tx_execution_info.summarize(VersionedConstants::latest_constants()).event_summary.n_events,
        n_execute_events
            + n_fee_transfer_events
            + n_execution_events
            + n_inner_calls
            + n_events_for_each_inner_call * n_inner_calls
    );
}

// This function gets a set of builtins for the outer and inner calls, updates the
// param builtin counter and returns the expected values for the summary test.
fn update_builtin_counters_for_summary_test(
    params: &mut TestExecutionSummary,
    outer_poseidon: usize,
    outer_bitwise: usize,
    inner_pedersen: usize,
    inner_bitwise: usize,
) -> (usize, usize, usize) {
    params.update_builtin_counters(&BuiltinCounterMap::from_iter([
        (BuiltinName::poseidon, outer_poseidon),
        (BuiltinName::bitwise, outer_bitwise),
    ]));

    params.update_inner_builtin_counters(&BuiltinCounterMap::from_iter([
        (BuiltinName::pedersen, inner_pedersen),
        (BuiltinName::bitwise, inner_bitwise),
    ]));
    (outer_poseidon, inner_pedersen, outer_bitwise + inner_bitwise)
}

#[rstest]
#[case(
    &mut TestExecutionSummary::new(10, 1, 2, class_hash!("0x1"), "0x1", "0x1"),
    &mut TestExecutionSummary::new(20, 2, 3, class_hash!("0x2"), "0x2", "0x2"),
    &mut TestExecutionSummary::new(30, 3, 4, class_hash!("0x3"), "0x3", "0x3")
)]
fn test_summarize(
    #[case] validate_params: &mut TestExecutionSummary,
    #[case] execute_params: &mut TestExecutionSummary,
    #[case] fee_transfer_params: &mut TestExecutionSummary,
) {
    let (validate_poseidon, validate_pedersen, validate_bitwise) =
        update_builtin_counters_for_summary_test(validate_params, 1, 5, 2, 6);

    let (execute_poseidon, execute_pedersen, execute_bitwise) =
        update_builtin_counters_for_summary_test(execute_params, 1, 4, 2, 1);

    let (_fee_transfer_poseidon, _fee_transfer_pedersen, _fee_transfer_bitwise) =
        update_builtin_counters_for_summary_test(fee_transfer_params, 1, 2, 3, 4);

    let validate_call_info = validate_params.to_call_info();
    let execute_call_info = execute_params.to_call_info();
    let fee_transfer_call_info = fee_transfer_params.to_call_info();

    let tx_execution_info = TransactionExecutionInfo {
        validate_call_info: Some(validate_call_info),
        execute_call_info: Some(execute_call_info),
        fee_transfer_call_info: Some(fee_transfer_call_info),
        ..Default::default()
    };

    let expected_summary = ExecutionSummary {
        charged_resources: ChargedResources {
            gas_consumed: validate_params.gas_consumed
                + execute_params.gas_consumed
                + fee_transfer_params.gas_consumed,
            ..Default::default()
        },
        executed_class_hashes: vec![
            validate_params.class_hash,
            execute_params.class_hash,
            fee_transfer_params.class_hash,
        ]
        .into_iter()
        .collect(),
        visited_storage_entries: vec![
            (validate_params.storage_address, validate_params.storage_key),
            (execute_params.storage_address, execute_params.storage_key),
            (fee_transfer_params.storage_address, fee_transfer_params.storage_key),
        ]
        .into_iter()
        .collect(),
        l2_to_l1_payload_lengths: vec![
            1;
            validate_params.num_of_messages
                + execute_params.num_of_messages
                + fee_transfer_params.num_of_messages
        ],
        event_summary: EventSummary {
            n_events: validate_params.num_of_events
                + execute_params.num_of_events
                + fee_transfer_params.num_of_events,
            total_event_keys: 0,
            total_event_data_size: 0,
        },
        number_of_calls: NumberOfCalls { n_calls: 6, n_calls_run_native: 0 },
    };

    // Omit the fee transfer builtin counters as done in `summarize_builtins`.
    let expected_builtins = BuiltinCounterMap::from_iter([
        (BuiltinName::pedersen, validate_pedersen + execute_pedersen),
        (BuiltinName::poseidon, validate_poseidon + execute_poseidon),
        (BuiltinName::bitwise, validate_bitwise + execute_bitwise),
    ]);

    // Call the summarize method.
    let actual_summary = tx_execution_info.summarize(VersionedConstants::latest_constants());
    let actual_builtins = tx_execution_info.summarize_builtins();

    // Compare the actual result with the expected result.
    assert_eq!(actual_summary, expected_summary);
    assert_eq!(actual_builtins, expected_builtins);
}
