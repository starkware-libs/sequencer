use std::collections::HashSet;

use blockifier::blockifier_versioned_constants::{
    RawVersionedConstants,
    ResourcesParams,
    VariableCallDataFactor,
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
use starknet_types_core::felt::Felt;
use strum::IntoEnumIterator;

use crate::initial_state::create_default_initial_state_data;
use crate::test_manager::{EventPredicateExpectation, TestBuilder, TestBuilderConfig};
use crate::tests::NON_TRIVIAL_RESOURCE_BOUNDS;
use crate::utils::get_class_hash_of_feature_contract;

// TODO(Dori): Delete this, or at least reduce it to a minimal set of unmeasurable syscalls.
const UNMEASURABLE_SYSCALLS: [Selector; 30] = [
    Selector::DelegateCall,
    Selector::DelegateL1Handler,
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

const SYSCALLS_WITH_LINEAR_FACTOR: [Selector; 2] = [Selector::Deploy, Selector::MetaTxV0];

/// Expected syscalls in the fee transfer call. Should be removed from the list of syscalls during
/// measurement iteration - only the syscalls called during __execute__ should be measured.
const FEE_TRANSFER_SYSCALLS: [Selector; 10] = [
    Selector::GetExecutionInfo,
    Selector::StorageRead,
    Selector::StorageRead,
    Selector::StorageWrite,
    Selector::StorageWrite,
    Selector::StorageRead,
    Selector::StorageRead,
    Selector::StorageWrite,
    Selector::StorageWrite,
    Selector::EmitEvent,
];

#[tokio::test]
async fn test_fee_transfer_syscalls() {
    let os_resources_contract = FeatureContract::OsResourcesTest(RunnableCairo1::Casm);
    let (mut builder, [os_resources_contract_address]) =
        TestBuilder::create_standard([(os_resources_contract, calldata![Felt::ZERO])]).await;

    // Fund the contract - it will be used as the account.
    // Then, move on to the next block, so the syscall-measurement tx is in it's own block.
    builder.add_fund_address_tx_with_default_amount(os_resources_contract_address);

    // Invoke from the OS resources contract, with zeros as calldata, to make the __execute__ do
    // nothing. All resulting events should be from the fee transfer call.
    builder.add_invoke_tx(
        InvokeTransaction::create(
            invoke_tx(invoke_tx_args! {
                sender_address: os_resources_contract_address,
                calldata: calldata![Felt::ZERO, Felt::ZERO, Felt::ZERO],
                resource_bounds: *NON_TRIVIAL_RESOURCE_BOUNDS,
            }),
            &builder.chain_id(),
        )
        .unwrap(),
        None,
        None,
    );

    // Build, run, and get the syscalls list.
    let test_output = builder.build_and_run().await;
    let syscalls = test_output
        .runner_output
        .txs_trace
        .last()
        .unwrap()
        .get_syscalls()
        .iter()
        .map(|syscall_trace| syscall_trace.get_selector())
        .collect::<Vec<_>>();
    assert_eq!(syscalls, FEE_TRANSFER_SYSCALLS.to_vec());
}

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
            calldata![Felt::ZERO],
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
            calldata: calldata![
                *os_resources_class_hash, **os_resources_contract_address, Felt::ZERO
            ],
            resource_bounds: *NON_TRIVIAL_RESOURCE_BOUNDS,
        }),
        &test_builder.chain_id(),
    )
    .unwrap();
    test_builder.add_invoke_tx(
        tx,
        None,
        // Expect one event from the emit-event syscall measurement.
        Some(vec![EventPredicateExpectation {
            description: "emit event syscall".to_string(),
            predicate: Box::new(move |event| {
                event.from_address == os_resources_contract_address
                    && event.content.keys[0].0 == Felt::from(5)
                    && event.content.data.0[0] == Felt::from(7)
            }),
        }]),
    );

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
    // Remove the fee transfer syscalls from the list by splitting the iterator into two. The second
    // part is the last `FEE_TRANSFER_SYSCALLS.len()` syscalls, and the first part should be the
    // rest.
    let all_syscalls = test_output.runner_output.txs_trace.last().unwrap().get_syscalls().clone();
    let (syscall_traces, fee_transfer_syscall_traces) =
        all_syscalls.split_at(all_syscalls.len() - FEE_TRANSFER_SYSCALLS.len());
    assert_eq!(
        fee_transfer_syscall_traces
            .iter()
            .map(|syscall_trace| syscall_trace.get_selector())
            .collect::<Vec<_>>(),
        FEE_TRANSFER_SYSCALLS.to_vec()
    );

    // Measure each syscall overhead. If the syscall incurs an inner call, subtract the inner call
    // overhead.
    let mut inner_calls_iter = inner_calls.into_iter();
    let mut syscalls_iter = syscall_traces
        .into_iter()
        .filter(|syscall_trace| !UNMEASURABLE_SYSCALLS.contains(&syscall_trace.get_selector()));
    let mut measurements: IndexMap<Selector, VariableResourceParams> = IndexMap::new();
    let mut fetch_inner_resources = |selector: Selector| -> ExecutionResources {
        if selector.is_calling_syscall() {
            // TODO(Dori): Consider supporting memory-hole counting in the OsLogger. Until then, we
            //   cannot subtract inner calls with positive memory-hole counts from the OsLogger
            //   resources.
            let mut inner_resources = inner_calls_iter.next().unwrap().resources.vm_resources;
            inner_resources.n_memory_holes = 0;
            inner_resources
        } else {
            ExecutionResources::default()
        }
    };
    while let Some(syscall_trace) = syscalls_iter.next() {
        let selector = syscall_trace.get_selector();

        // Ensure we don't visit the same syscall more than once.
        assert!(
            measurements.get(&selector).is_none(),
            "Syscall {selector:?} was visited again, unexpectedly."
        );

        // If this syscall incurs an inner call, it should be the next inner call in the
        // iterator.
        let inner_overhead = fetch_inner_resources(selector);
        // The resources measured here are one of two types: constant, or base cost of a syscall
        // with a linear factor.
        let resources =
            (syscall_trace.get_resources().unwrap() - &inner_overhead).filter_unused_builtins();

        // If this if a syscall with a linear factor, the next syscall should be the linear cost.
        // Otherwise, this syscall has a constant cost.
        let syscall_cost = if SYSCALLS_WITH_LINEAR_FACTOR.contains(&selector) {
            let next_syscall_trace = syscalls_iter.next().unwrap();
            assert_eq!(
                selector,
                next_syscall_trace.get_selector(),
                "Expected next syscall to be the same as the current syscall {selector:?}, but \
                 got {:?}.",
                next_syscall_trace.get_selector()
            );
            let next_inner_overhead = fetch_inner_resources(selector);
            let next_resources = (next_syscall_trace.get_resources().unwrap()
                - &next_inner_overhead)
                .filter_unused_builtins();
            let linear_factor_resources = (&next_resources - &resources).filter_unused_builtins();
            VariableResourceParams::WithFactor(ResourcesParams {
                constant: resources,
                // Syscalls with a linear factor have an unscaled linear factor cost.
                calldata_factor: VariableCallDataFactor::Unscaled(linear_factor_resources),
            })
        } else {
            VariableResourceParams::Constant(resources)
        };

        measurements.insert(selector, syscall_cost);
    }

    // Make sure we covered all syscalls we expect to.
    assert_eq!(
        HashSet::from_iter(measurements.keys().cloned()),
        Selector::iter()
            .collect::<HashSet<_>>()
            .difference(&UNMEASURABLE_SYSCALLS.iter().cloned().collect::<HashSet<_>>())
            .copied()
            .collect::<HashSet<_>>()
    );

    // Make sure there are no more dangling syscalls.
    let dangling_syscall = syscalls_iter.next();
    assert!(
        dangling_syscall.is_none(),
        "There are more syscalls than expected. Dangling syscall: {dangling_syscall:?}."
    );

    // Compare the measurements with the expected values on the latest VC.
    let version = StarknetVersion::LATEST;
    let mut raw_vc: RawVersionedConstants =
        serde_json::from_str(VersionedConstants::json_str(&version).unwrap()).unwrap();
    for (syscall, resources) in measurements {
        raw_vc.os_resources.execute_syscalls.insert(syscall, resources);
    }
    expect_file![VersionedConstants::json_path(&version).unwrap()]
        .assert_eq(&raw_vc.to_string_pretty());
}
