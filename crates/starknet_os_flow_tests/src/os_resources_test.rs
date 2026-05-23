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
use starknet_api::transaction::fields::ContractAddressSalt;
use starknet_api::versioned_constants_logic::VersionedConstantsTrait;
use starknet_api::{calldata, declare_tx_args, invoke_tx_args};
use starknet_os::hint_processor::os_logger::ResourceFinalizer;
use starknet_types_core::felt::Felt;
use strum::IntoEnumIterator;

use crate::initial_state::{
    create_default_initial_state_data,
    get_deploy_contract_tx_and_address_with_salt_and_deployer,
};
use crate::special_contracts::{
    DEPLOYABLE_FOR_RESOURCE_MEASUREMENT_CONTRACT_CASM,
    DEPLOYABLE_FOR_RESOURCE_MEASUREMENT_CONTRACT_SIERRA,
};
use crate::test_manager::{
    EventPredicateExpectation,
    TestBuilder,
    TestBuilderConfig,
    FUNDED_ACCOUNT_ADDRESS,
};
use crate::tests::NON_TRIVIAL_RESOURCE_BOUNDS;

// TODO(Dori): Delete this, or at least reduce it to a minimal set of unmeasurable syscalls.
const UNMEASURABLE_SYSCALLS: [Selector; 31] = [
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
    Selector::Sha512ProcessBlock,
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

/// Measure the OS overhead for each syscall, and compare the results with the latest VC.
///
/// This test relies on the [starknet_os::hint_processor::os_logger::OsLogger] to capture the
/// resources used by the OS when running a syscall. A "checkpoint" is made before entering a
/// syscall implementation, and after the syscall execution returns, the difference between the two
/// is stored in the logger's traces.
///
/// Some notes about these measurements:
/// 1. Some syscalls incur inner calls ([Selector::CallContract], for example). The resources
///    consumed by the inner logic must be subtracted from the measured overhead to get the actual
///    OS overhead.
/// 2. Some syscalls incur overhead that depends linearly on the length of the input to the syscall.
///    In these cases, the measuring contract calls the syscall twice in a row, the second call
///    having "one more" input than the first call; subtracting the sequential measurements gives
///    the linear factor of the syscall. One caveat here is that the linear factor of
///    [Selector::Keccak] is stored as a separate syscall cost ([Selector::KeccakRound]).
/// 3. The SHA family syscalls are implemented as "virtual builtins": the syscall execution only
///    pushes the inputs to a special memory segment, and the "heavy lifting" is done later. This
///    means that the overhead of the syscall is not captured by the `OsLogger`. These syscalls have
///    separate tests to measure their overhead.
/// 4. The [Selector::Deploy] syscall's overhead depends on the deployed contract address in a non-
///    trivial way (see the `normalize_address` function in the cairo-lang core). To avoid noise in
///    the measurements, we use a stable dummy contract (that is not recompiled when the Cairo1
///    compiler's version changes), and we set the `deploy_from_zero` flag to `true` to make sure
///    changes in the deploying contract address are not reflected in the measurements.
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
    test_builder.add_fund_address_tx_with_default_amount(os_resources_contract_address);

    // Declare and deploy an instance of the stable contract.
    let stable_contract_sierra = &DEPLOYABLE_FOR_RESOURCE_MEASUREMENT_CONTRACT_SIERRA;
    let stable_contract_casm = &DEPLOYABLE_FOR_RESOURCE_MEASUREMENT_CONTRACT_CASM;
    let stable_contract_class_hash = stable_contract_sierra.calculate_class_hash();
    let extra_declare_args = declare_tx_args! {
        sender_address: *FUNDED_ACCOUNT_ADDRESS,
        nonce: test_builder.next_nonce(*FUNDED_ACCOUNT_ADDRESS),
        resource_bounds: *NON_TRIVIAL_RESOURCE_BOUNDS,
    };
    test_builder.add_explicit_cairo1_declare_tx(
        stable_contract_sierra,
        (**stable_contract_casm).clone(),
        extra_declare_args,
        &test_builder.chain_id(),
    );
    let deploy_from_zero = true;
    let (deploy_tx, stable_contract_address) =
        get_deploy_contract_tx_and_address_with_salt_and_deployer(
            stable_contract_class_hash,
            calldata![Felt::ZERO], // Ctor calldata length.
            test_builder.next_nonce(*FUNDED_ACCOUNT_ADDRESS),
            *NON_TRIVIAL_RESOURCE_BOUNDS,
            ContractAddressSalt::default(),
            deploy_from_zero,
        );
    test_builder.add_invoke_tx(deploy_tx, None, None);

    // Move on to the next block, so the syscall-measurement tx is in it's own block.
    test_builder.move_to_next_block();

    // Add the syscall-measurement tx.
    let tx = InvokeTransaction::create(
        invoke_tx(invoke_tx_args! {
            sender_address: os_resources_contract_address,
            calldata: calldata![*stable_contract_class_hash, **stable_contract_address],
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

    let mut inner_calls_iter = inner_calls.into_iter();

    // Extract syscall resources consumed per syscall.
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
    let mut syscalls_iter = syscall_traces.iter();
    let mut measurements: IndexMap<Selector, VariableResourceParams> = IndexMap::new();
    // If the syscall incurs an inner call, subtract the inner call overhead.
    let mut fetch_inner_resources = |selector: Selector| -> ExecutionResources {
        // We assume no inner calls have nested inner calls (all inner calls are leaves).
        if selector.is_calling_syscall() {
            // TODO(Dori): Take opcodes (like blake) into account, instead of using the vm_resources
            // field.
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
