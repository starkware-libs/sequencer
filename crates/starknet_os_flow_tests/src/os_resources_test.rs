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
use starknet_api::contract_class::compiled_class_hash::{HashVersion, HashableCompiledClass};
use starknet_api::contract_class::{ClassInfo, ContractClass, SierraVersion};
use starknet_api::executable_transaction::{DeclareTransaction, InvokeTransaction};
use starknet_api::test_utils::declare::declare_tx;
use starknet_api::test_utils::invoke::invoke_tx;
use starknet_api::versioned_constants_logic::VersionedConstantsTrait;
use starknet_api::{calldata, declare_tx_args, invoke_tx_args};
use starknet_os::hint_processor::os_logger::ResourceFinalizer;
use strum::IntoEnumIterator;

use crate::initial_state::create_default_initial_state_data;
use crate::special_contracts::{
    DEPLOYABLE_FOR_RESOURCE_MEASUREMENT_CONTRACT_CASM,
    DEPLOYABLE_FOR_RESOURCE_MEASUREMENT_CONTRACT_SIERRA,
};
use crate::test_manager::{TestBuilder, TestBuilderConfig, FUNDED_ACCOUNT_ADDRESS};
use crate::tests::NON_TRIVIAL_RESOURCE_BOUNDS;
use crate::utils::get_class_hash_of_feature_contract;

// TODO(Dori): Delete this, or at least reduce it to a minimal set of unmeasurable syscalls.
const UNMEASURABLE_SYSCALLS: [Selector; 32] = [
    Selector::DelegateCall,
    Selector::DelegateL1Handler,
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

const SYSCALLS_WITH_LINEAR_FACTOR: [Selector; 2] = [Selector::Deploy, Selector::MetaTxV0];

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
    test_builder.add_fund_address_tx_with_default_amount(os_resources_contract_address);

    // Declare the deployable contract, so it can be deployed later.
    let deployable_contract_sierra = &DEPLOYABLE_FOR_RESOURCE_MEASUREMENT_CONTRACT_SIERRA;
    let deployable_contract_casm = &DEPLOYABLE_FOR_RESOURCE_MEASUREMENT_CONTRACT_CASM;
    let deployable_class_hash = deployable_contract_sierra.calculate_class_hash();
    let deployable_compiled_class_hash = deployable_contract_casm.hash(&HashVersion::V2);
    let declare_args = declare_tx_args! {
        sender_address: *FUNDED_ACCOUNT_ADDRESS,
        nonce: test_builder.next_nonce(*FUNDED_ACCOUNT_ADDRESS),
        class_hash: deployable_class_hash,
        compiled_class_hash: deployable_compiled_class_hash,
        resource_bounds: *NON_TRIVIAL_RESOURCE_BOUNDS,
    };
    let account_declare_tx = declare_tx(declare_args);
    let sierra_version = deployable_contract_sierra.get_sierra_version().unwrap();
    let class_info = ClassInfo {
        contract_class: ContractClass::V1((
            (**deployable_contract_casm).clone(),
            sierra_version.clone(),
        )),
        sierra_program_length: deployable_contract_sierra.sierra_program.len(),
        abi_length: deployable_contract_sierra.abi.len(),
        sierra_version,
    };
    let tx = DeclareTransaction::create(account_declare_tx, class_info, &test_builder.chain_id())
        .unwrap();
    test_builder.add_cairo1_declare_tx(tx, deployable_contract_sierra);

    // Move on to the next block, so the syscall-measurement tx is in it's own block.
    test_builder.move_to_next_block();

    // Add the syscall-measurement tx.
    let tx = InvokeTransaction::create(
        invoke_tx(invoke_tx_args! {
            sender_address: os_resources_contract_address,
            calldata: calldata![
                *os_resources_class_hash, **os_resources_contract_address, *deployable_class_hash
            ],
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
    let mut syscalls_iter = syscall_traces
        .iter()
        .filter(|syscall_trace| !UNMEASURABLE_SYSCALLS.contains(&syscall_trace.get_selector()));
    let mut measurements: IndexMap<Selector, VariableResourceParams> = IndexMap::new();
    let mut fetch_inner_resources = |selector: Selector| -> ExecutionResources {
        if selector.is_calling_syscall() {
            inner_calls_iter.next().unwrap().resources.vm_resources
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
