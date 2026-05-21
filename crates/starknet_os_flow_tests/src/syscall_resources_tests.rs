use std::collections::HashMap;

use blockifier::execution::syscalls::vm_syscall_utils::{SyscallSelector, SyscallUsage};
use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::{calldata, invoke_tx_args};
use starknet_types_core::felt::Felt;

use starknet_os::hint_processor::os_logger::ResourceFinalizer;

use crate::test_manager::TestBuilder;

// From Python os_resources_test.py: STEPS_FOR_RETURNING_FROM_INNER_SYSCALL.
// Steps consumed after OsLoggerExitSyscall that are not captured in the logger's step span,
// for call-type syscalls (CallContract, LibraryCall).
const STEPS_FOR_RETURNING_FROM_INNER_SYSCALL: usize = 8;

#[tokio::test]
async fn test_call_contract_syscall_resources() {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let (mut test_builder, [caller_address, callee_address]) =
        TestBuilder::<DictStateReader>::create_standard([
            (test_contract, calldata![Felt::ZERO, Felt::ZERO]),
            (test_contract, calldata![Felt::ZERO, Felt::ZERO]),
        ])
        .await;

    // Use `empty` (no arguments, no syscalls, empty body) as the callee so the inner CC OS
    // step count reflects the pure CallContract handler cost — matching the Python
    // EMPTY_FUNCTION_SELECTOR approach. Pass an empty calldata array (length = 0).
    let calldata = create_calldata(
        caller_address,
        "test_call_contract",
        &[
            **callee_address,
            selector_from_name("empty").0,
            Felt::ZERO, // calldata array length = 0
        ],
    );
    test_builder.add_funded_account_invoke(invoke_tx_args! { calldata });

    let versioned_constants =
        test_builder.initial_state.block_context.versioned_constants().clone();
    let test_runner = test_builder.build().await;
    let test_output = test_runner.run();

    let tx_trace = &test_output.runner_output.os_logger.get_txs()[0];
    // The outer CC is the first CallContract at the transaction level (account → caller).
    let outer_cc = tx_trace
        .syscalls
        .iter()
        .find(|syscall| syscall.selector() == SyscallSelector::CallContract)
        .expect("outer CC syscall not found in tx trace");
    // The inner CC is the first CallContract nested inside the outer one (caller → callee).
    let inner_cc = outer_cc
        .inner_syscalls()
        .iter()
        .find(|syscall| syscall.selector() == SyscallSelector::CallContract)
        .expect("inner CC syscall not found in outer CC trace");

    let inner_cc_resources = inner_cc.get_resources().unwrap();
    let inner_cc_steps = inner_cc_resources.n_steps;
    let inner_cc_range_checks = inner_cc_resources
        .builtin_instance_counter
        .get(&cairo_vm::types::builtin_name::BuiltinName::range_check)
        .copied()
        .unwrap_or(0);
    eprintln!("inner_cc: os_steps={inner_cc_steps}, range_check={inner_cc_range_checks}");
    let measured_overhead = inner_cc_steps + STEPS_FOR_RETURNING_FROM_INNER_SYSCALL;

    let expected_resources = versioned_constants.get_additional_os_syscall_resources(
        &HashMap::from([(SyscallSelector::CallContract, SyscallUsage::with_call_count(1))]),
    );
    assert_eq!(measured_overhead, expected_resources.n_steps);
}
