use std::sync::LazyLock;

use blockifier::execution::call_info::{CallExecution, CallInfo, OrderedEvent};
use blockifier::execution::entry_point::CallEntryPoint;
use blockifier::transaction::objects::TransactionExecutionInfo;
use rstest::rstest;
use starknet_api::core::ContractAddress;
use starknet_api::transaction::{Event, EventContent, EventData, EventKey};
use starknet_api::{contract_address, felt};

use super::get_events_from_execution_info;

fn event_content(identifier: u32) -> EventContent {
    EventContent {
        keys: vec![EventKey(felt!(identifier))],
        data: EventData(vec![felt!(identifier)]),
    }
}

fn event(address: ContractAddress, identifier: u32) -> Event {
    Event { from_address: address, content: event_content(identifier) }
}

fn ordered_event(order: usize, identifier: u32) -> OrderedEvent {
    OrderedEvent { order, event: event_content(identifier) }
}

fn create_call_info_with_events(
    events: Vec<OrderedEvent>,
    storage_address: ContractAddress,
) -> CallInfo {
    CallInfo {
        call: CallEntryPoint { storage_address, ..Default::default() },
        execution: CallExecution { events, ..Default::default() },
        ..Default::default()
    }
}

static ACCOUNT_ADDRESS: LazyLock<ContractAddress> = LazyLock::new(|| contract_address!("0x1"));
static CALLED_CONTRACT_ADDRESS: LazyLock<ContractAddress> =
    LazyLock::new(|| contract_address!("0x2"));

struct TestArgs {
    test_args_input: TestArgsInput,
    test_args_expectation: TestArgsExpectation,
}

struct TestArgsExpectation {
    encoded_events: Vec<(ContractAddress, u32)>,
}

impl TestArgsExpectation {
    fn events(&self) -> Vec<Event> {
        self.encoded_events
            .iter()
            .map(|(address, identifier)| event(*address, *identifier))
            .collect()
    }
}

fn encoded_events_to_ordered_events(encoded_events: &[(usize, u32)]) -> Vec<OrderedEvent> {
    encoded_events.iter().map(|(order, identifier)| ordered_event(*order, *identifier)).collect()
}

struct TestArgsInput {
    encoded_validate_events: Vec<(usize, u32)>,
    encoded_execute_events: Vec<(usize, u32)>,
    encoded_inner_call_events: Option<Vec<(usize, u32)>>,
}

impl TestArgsInput {
    fn validate_call_info(&self) -> CallInfo {
        create_call_info_with_events(
            encoded_events_to_ordered_events(&self.encoded_validate_events),
            *ACCOUNT_ADDRESS,
        )
    }

    fn execute_call_info(&self) -> CallInfo {
        let inner_calls = if let Some(encoded_inner_call_events) = &self.encoded_inner_call_events {
            vec![create_call_info_with_events(
                encoded_events_to_ordered_events(encoded_inner_call_events),
                *CALLED_CONTRACT_ADDRESS,
            )]
        } else {
            vec![]
        };

        CallInfo {
            call: CallEntryPoint { storage_address: *ACCOUNT_ADDRESS, ..Default::default() },
            execution: CallExecution {
                events: encoded_events_to_ordered_events(&self.encoded_execute_events),
                ..Default::default()
            },
            inner_calls,
            ..Default::default()
        }
    }

    fn execution_info(&self) -> TransactionExecutionInfo {
        TransactionExecutionInfo {
            validate_call_info: Some(self.validate_call_info()),
            execute_call_info: Some(self.execute_call_info()),
            fee_transfer_call_info: Some(CallInfo::default()),
            ..Default::default()
        }
    }
}

#[rstest]
#[case::sorter_per_main_call_info(TestArgs {
    test_args_input: TestArgsInput {
        encoded_validate_events: vec![(1, 101), (0, 100), (2, 102)],
        encoded_execute_events: vec![(2, 202), (0, 200), (1, 201), ],
        encoded_inner_call_events: None,
    },
    test_args_expectation: TestArgsExpectation {
        encoded_events: vec![
            (*ACCOUNT_ADDRESS, 100),
            (*ACCOUNT_ADDRESS, 101),
            (*ACCOUNT_ADDRESS, 102),
            (*ACCOUNT_ADDRESS, 200),
            (*ACCOUNT_ADDRESS, 201),
            (*ACCOUNT_ADDRESS, 202),
        ],
    },
})]
#[case::with_inner_calls(TestArgs {
    test_args_input: TestArgsInput {
        encoded_validate_events: vec![(0, 100), (1, 101)],
        encoded_execute_events: vec![(0, 200), (2, 202), (3, 203)],
        encoded_inner_call_events: Some(vec![(1, 201), (4, 204)]),
    },
    test_args_expectation: TestArgsExpectation {
        encoded_events: vec![
            (*ACCOUNT_ADDRESS, 100),
            (*ACCOUNT_ADDRESS, 101),
            (*ACCOUNT_ADDRESS, 200),
            (*CALLED_CONTRACT_ADDRESS, 201),
            (*ACCOUNT_ADDRESS, 202),
            (*ACCOUNT_ADDRESS, 203),
            (*CALLED_CONTRACT_ADDRESS, 204),
        ],
    },
})]
fn test_get_events_from_execution_info(#[case] test_args: TestArgs) {
    let execution_info = test_args.test_args_input.execution_info();
    let events = get_events_from_execution_info(&execution_info);
    assert_eq!(events, test_args.test_args_expectation.events());
}
