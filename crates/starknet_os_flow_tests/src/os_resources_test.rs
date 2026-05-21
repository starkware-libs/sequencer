use std::collections::HashSet;

use blockifier::blockifier_versioned_constants::{
    RawVersionedConstants,
    VariableResourceParams,
    VersionedConstants,
};
use blockifier::context::BlockContext;
use blockifier::execution::deprecated_syscalls::DeprecatedSyscallSelector as Selector;
use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier_test_utils::cairo_versions::RunnableCairo1;
use blockifier_test_utils::contracts::FeatureContract;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use expect_test::expect_file;
use indexmap::IndexMap;
use starknet_api::block::StarknetVersion;
use starknet_api::contract_class::SierraVersion;
use starknet_api::executable_transaction::InvokeTransaction;
use starknet_api::test_utils::invoke::invoke_tx;
use starknet_api::versioned_constants_logic::VersionedConstantsTrait;
use starknet_api::{calldata, invoke_tx_args};
use starknet_os::hint_processor::os_logger::ResourceFinalizer;
use strum::IntoEnumIterator;

use crate::initial_state::create_default_initial_state_data;
use crate::test_manager::{TestBuilder, TestBuilderConfig};
use crate::tests::NON_TRIVIAL_RESOURCE_BOUNDS;
use crate::utils::get_class_hash_of_feature_contract;

// TODO(Dori): Delete this, or at least reduce it to a minimal set of unmeasurable syscalls.
const UNMEASURABLE_SYSCALLS: [Selector; 33] = [
    Selector::DelegateCall,
    Selector::DelegateL1Handler,
    Selector::Deploy,
    Selector::EmitEvent,
    Selector::GetBlockHash,
    Selector::GetBlockNumber,
    Selector::GetBlockTimestamp,
    Selector::GetCallerAddress,
    Selector::GetClassHashAt,
    Selector::GetContractAddress,
    Selector::GetExecutionInfo,
    Selector::GetSequencerAddress,
    Selector::GetTxInfo,
    Selector::GetTxSignature,
    Selector::Keccak,
    Selector::KeccakRound,
    Selector::Sha256ProcessBlock,
    Selector::LibraryCallL1Handler,
    Selector::MetaTxV0,
    Selector::ReplaceClass,
    Selector::Secp256k1Add,
    Selector::Secp256k1GetPointFromX,
    Selector::Secp256k1GetXy,
    Selector::Secp256k1Mul,
    Selector::Secp256k1New,
    Selector::Secp256r1Add,
    Selector::Secp256r1GetPointFromX,
    Selector::Secp256r1GetXy,
    Selector::Secp256r1Mul,
    Selector::Secp256r1New,
    Selector::SendMessageToL1,
    Selector::StorageRead,
    Selector::StorageWrite,
];

#[tokio::test]
async fn test_os_resources_regression() {
    let os_resources_contract = FeatureContract::OsResourcesTest(RunnableCairo1::Casm);
    let os_resources_class_hash = get_class_hash_of_feature_contract(os_resources_contract);

    // Setup the test initial state and test builder.
    // Need to explicitly set up the state to be able to override the minimal sierra version for gas
    // tracking, in order to force step tracking mode.
    let (mut initial_state_data, [os_resources_contract_address]) =
        create_default_initial_state_data::<DictStateReader, 1>([(
            os_resources_contract,
            calldata![],
        )])
        .await;
    initial_state_data.initial_state.block_context = {
        let block_context = &initial_state_data.initial_state.block_context;
        let mut vc = block_context.versioned_constants().clone();
        vc.min_sierra_version_for_sierra_gas = SierraVersion::new(99, 99, 99);
        BlockContext::new(
            block_context.block_info().clone(),
            block_context.chain_info().clone(),
            vc,
            block_context.bouncer_config.clone(),
        )
    };
    let virtual_os = false;
    let mut test_builder = TestBuilder::new_with_initial_state_data(
        initial_state_data,
        TestBuilderConfig::default(),
        virtual_os,
    );

    // Fund the contract - it will be used as the account.
    // Then, move on to the next block, so the syscall-measurement tx is in it's own block.
    test_builder.add_fund_address_tx_with_default_amount(os_resources_contract_address);
    test_builder.move_to_next_block();

    // Add the syscall-measurement tx.
    let tx = InvokeTransaction::create(
        invoke_tx(invoke_tx_args! {
            sender_address: os_resources_contract_address,
            calldata: calldata![*os_resources_class_hash, **os_resources_contract_address],
            resource_bounds: *NON_TRIVIAL_RESOURCE_BOUNDS,
        }),
        &test_builder.chain_id(),
    )
    .unwrap();
    test_builder.add_invoke_tx(tx, None, None);

    // Run test. Grab the execution info from the runner (for later) before consuming it.
    let test_runner = test_builder.build().await;
    let inner_calls = test_runner
        .os_hints
        .os_input
        .os_block_inputs
        .last()
        .unwrap()
        .tx_execution_infos
        .last()
        .unwrap()
        .execute_call_info
        .as_ref()
        .unwrap()
        .inner_calls
        .clone();
    let test_output = test_runner.run();
    test_output.perform_default_validations();

    // Extract syscall resources consumed, per (measurable) syscall.
    let syscall_traces = test_output.runner_output.txs_trace.last().unwrap().get_syscalls();

    // Measure each syscall overhead. If the syscall incurs an inner call, subtract the inner call
    // overhead.
    let mut inner_calls_iter = inner_calls.into_iter();
    let mut visited_syscalls = HashSet::new();
    let measurements: IndexMap<Selector, ExecutionResources> = syscall_traces
        .iter()
        .filter_map(|syscall_trace| {
            let selector = syscall_trace.get_selector();
            if UNMEASURABLE_SYSCALLS.contains(&selector) {
                return None;
            }

            // Ensure we don't visit the same syscall twice.
            assert!(
                !visited_syscalls.contains(&selector),
                "Syscall {selector:?} was visited twice."
            );
            visited_syscalls.insert(selector);

            // If this syscall incurs an inner call, it should be the next inner call in the
            // iterator.
            let inner_overhead = if selector.is_calling_syscall() {
                inner_calls_iter.next().unwrap().resources.vm_resources
            } else {
                ExecutionResources::default()
            };

            Some((
                selector,
                (syscall_trace.get_resources().unwrap() - &inner_overhead).filter_unused_builtins(),
            ))
        })
        .collect();

    // Make sure we covered all syscalls we expect to.
    assert_eq!(
        visited_syscalls,
        Selector::iter()
            .collect::<HashSet<_>>()
            .difference(&UNMEASURABLE_SYSCALLS.iter().cloned().collect::<HashSet<_>>())
            .copied()
            .collect::<HashSet<_>>()
    );

    // Compare the measurements with the expected values on the latest VC.
    let version = StarknetVersion::LATEST;
    let mut raw_vc: RawVersionedConstants =
        serde_json::from_str(VersionedConstants::json_str(&version).unwrap()).unwrap();
    for (syscall, resources) in measurements {
        raw_vc
            .os_resources
            .execute_syscalls
            .insert(syscall, VariableResourceParams::Constant(resources));
    }
    expect_file![VersionedConstants::json_path(&version).unwrap()]
        .assert_eq(&raw_vc.to_string_pretty());
}
