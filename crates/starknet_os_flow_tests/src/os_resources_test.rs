use std::collections::HashSet;

use assert_matches::assert_matches;
use blockifier::blockifier_versioned_constants::{
    RawStepGasCost,
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
use starknet_api::core::{ClassHash, ContractAddress, EthAddress, Nonce};
use starknet_api::executable_transaction::{
    DeployAccountTransaction,
    InvokeTransaction,
    TransactionType,
};
use starknet_api::test_utils::deploy_account::deploy_account_tx;
use starknet_api::test_utils::invoke::invoke_tx;
use starknet_api::transaction::fields::ContractAddressSalt;
use starknet_api::transaction::{L2ToL1Payload, MessageToL1};
use starknet_api::versioned_constants_logic::VersionedConstantsTrait;
use starknet_api::{calldata, declare_tx_args, deploy_account_tx_args, invoke_tx_args};
use starknet_os::hint_processor::constants::BUILTIN_INSTANCE_SIZES;
use starknet_os::hint_processor::os_logger::ResourceFinalizer;
use starknet_os::test_utils::{SHA256_BATCH_RESOURCES_LINEAR, SHA256_BLOCK_TO_ROUND};
use starknet_types_core::felt::Felt;
use strum::IntoEnumIterator;

use crate::initial_state::{
    create_default_initial_state_data,
    get_deploy_contract_tx_and_address_with_salt_and_deployer,
};
use crate::special_contracts::{
    DATA_GAS_ACCOUNT_CONTRACT_CASM,
    DATA_GAS_ACCOUNT_CONTRACT_SIERRA,
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
const UNMEASURABLE_SYSCALLS: [Selector; 13] = [
    Selector::DelegateCall,
    Selector::DelegateL1Handler,
    Selector::GetBlockNumber,
    Selector::GetBlockTimestamp,
    Selector::GetCallerAddress,
    Selector::GetContractAddress,
    Selector::GetSequencerAddress,
    Selector::GetTxInfo,
    Selector::GetTxSignature,
    Selector::Sha512ProcessBlock,
    Selector::LibraryCallL1Handler,
    Selector::StorageRead,
    Selector::StorageWrite,
];

/// Keccak does not store the linear factor in the same entry in the versioned constants, but it
/// does have a measurable linear factor stored under [Selector::KeccakRound].
const SYSCALLS_WITH_LINEAR_FACTOR: [Selector; 3] =
    [Selector::Deploy, Selector::Keccak, Selector::MetaTxV0];

/// Syscalls that are implemented using virtual builtins. Such syscalls have their "heavy lifting"
/// executed after the execute_syscalls part of the OS, so the consumed resources are not captured
/// by the OsLogger.
const SYSCALLS_WITH_VIRTUAL_BUILTINS: [Selector; 1] = [Selector::Sha256ProcessBlock];

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

/// All other syscalls are called only once.
const SYSCALLS_CALLED_TWICE: [Selector; 2] = [Selector::Secp256k1New, Selector::Secp256r1New];

struct OsResourcesTestSetup {
    os_resources_contract_address: ContractAddress,
    stable_contract_address: ContractAddress,
    stable_contract_class_hash: ClassHash,
    test_builder: TestBuilder<DictStateReader>,
}

/// See [SYSCALLS_WITH_VIRTUAL_BUILTINS] for why this function is needed.
fn update_resources_for_virtual_builtin_syscall(
    selector: Selector,
    measured_base: ExecutionResources,
) -> ExecutionResources {
    assert!(SYSCALLS_WITH_VIRTUAL_BUILTINS.contains(&selector));
    match selector {
        Selector::Sha256ProcessBlock => {
            let mut new_resources = measured_base.clone();
            let ExecutionResources {
                n_steps: linear_steps,
                builtin_instance_counter: linear_builtin_instance_counter,
                n_memory_holes: linear_memory_holes,
            } = SHA256_BATCH_RESOURCES_LINEAR.clone();
            new_resources.n_steps += linear_steps / SHA256_BLOCK_TO_ROUND;
            new_resources.n_memory_holes += linear_memory_holes / SHA256_BLOCK_TO_ROUND;
            for (builtin, count) in linear_builtin_instance_counter.iter() {
                *new_resources.builtin_instance_counter.entry(*builtin).or_insert(0) +=
                    BUILTIN_INSTANCE_SIZES.get(builtin).unwrap() * count / SHA256_BLOCK_TO_ROUND;
            }
            new_resources
        }
        _ => panic!("Resource update not implemented for virtual builtin syscall: {selector:?}."),
    }
}

/// Setup an test builder with
/// 1. the OS-resources contract deployed,
/// 2. funded (it is an account contract as well),
/// 3. the initial block context (and therefore subsequent block contexts) with the minimal sierra
///    version for gas tracking set to "infinity", in order to force step tracking mode, and
/// 4. the stable contract declared and deployed.
async fn setup_test_builder() -> OsResourcesTestSetup {
    // Setup the test initial state and test builder.
    // Need to explicitly set up the state to be able to override the minimal sierra version for gas
    // tracking, in order to force step tracking mode.
    let os_resources_contract = FeatureContract::OsResourcesTest(RunnableCairo1::Casm);
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
    test_builder.add_fund_address_tx_with_default_amount(os_resources_contract_address);

    // Declare and deploy an instance of the stable contract. Also, fund it.
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
    test_builder.add_fund_address_tx_with_default_amount(stable_contract_address);

    // Move on to the next block, so the measurement txs are in their own block.
    test_builder.move_to_next_block();

    OsResourcesTestSetup {
        os_resources_contract_address,
        stable_contract_address,
        stable_contract_class_hash,
        test_builder,
    }
}

#[tokio::test]
async fn test_fee_transfer_syscalls() {
    let OsResourcesTestSetup { os_resources_contract_address, test_builder: mut builder, .. } =
        setup_test_builder().await;

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
async fn test_os_resources_regression() {
    let version = StarknetVersion::LATEST;
    let mut raw_vc: RawVersionedConstants =
        serde_json::from_str(VersionedConstants::json_str(&version).unwrap()).unwrap();

    let OsResourcesTestSetup {
        os_resources_contract_address,
        stable_contract_address,
        stable_contract_class_hash,
        mut test_builder,
    } = setup_test_builder().await;

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

    // Add the expected message to L1.
    test_builder.messages_to_l1.push(MessageToL1 {
        from_address: os_resources_contract_address,
        to_address: EthAddress::try_from(Felt::from(100)).unwrap(),
        payload: L2ToL1Payload(vec![]),
    });

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
    let mut second_visit_syscalls = HashSet::new();
    let mut inner_calls_iter = inner_calls.into_iter();
    let mut syscalls_iter = syscall_traces
        .iter()
        .filter(|syscall_trace| !UNMEASURABLE_SYSCALLS.contains(&syscall_trace.get_selector()));
    let mut measurements: IndexMap<Selector, VariableResourceParams> = IndexMap::new();
    let mut fetch_inner_resources = |selector: Selector| -> ExecutionResources {
        if selector.is_calling_syscall() {
            // TODO(Dori): Take opcodes (like blake) into account.
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

        // Ensure we don't visit the same syscall more than the allowed number of times.
        if measurements.get(&selector).is_some() {
            assert!(
                SYSCALLS_CALLED_TWICE.contains(&selector),
                "Syscall {selector:?} was visited again, unexpectedly."
            );
            assert!(
                !second_visit_syscalls.contains(&selector),
                "Syscall {selector:?} was visited a third time, unexpectedly."
            );
            second_visit_syscalls.insert(selector);
            continue;
        }

        // If this syscall incurs an inner call, it should be the next inner call in the
        // iterator.
        let inner_overhead = fetch_inner_resources(selector);
        // The resources measured here are one of two types: constant, or base cost of a syscall
        // with a linear factor.
        let mut resources =
            (syscall_trace.get_resources().unwrap() - &inner_overhead).filter_unused_builtins();

        // Virtual builtins require adjustment.
        if SYSCALLS_WITH_VIRTUAL_BUILTINS.contains(&selector) {
            resources = update_resources_for_virtual_builtin_syscall(selector, resources);
        }

        // If this if a syscall with a linear factor, the next syscall should be the linear cost.
        // Otherwise, this syscall has a constant cost.
        if SYSCALLS_WITH_LINEAR_FACTOR.contains(&selector) {
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
            // Keccak is a special case - we store the linear cost as a separate syscall.
            if selector == Selector::Keccak {
                // TODO(Dori): Currently, the Keccak base cost is enforced in the OS to equal the
                //   syscall base cost. If and when this is no longer the case, no need to replace
                //   `resources` (measured keccak base cost) with the syscall base cost, and no need
                //   to recompute the linear factor.
                let RawStepGasCost { step_gas_cost: n_steps } =
                    raw_vc.os_constants.syscall_base_gas_cost.clone();
                let resources = ExecutionResources {
                    n_steps: n_steps.0.try_into().unwrap(),
                    ..Default::default()
                };
                let linear_factor_resources =
                    (&next_resources - &resources).filter_unused_builtins();
                measurements.insert(Selector::Keccak, VariableResourceParams::Constant(resources));
                measurements.insert(
                    Selector::KeccakRound,
                    VariableResourceParams::Constant(linear_factor_resources),
                );
            } else {
                measurements.insert(
                    selector,
                    VariableResourceParams::WithFactor(ResourcesParams {
                        constant: resources,
                        // Syscalls with a linear factor have an unscaled linear factor cost.
                        calldata_factor: VariableCallDataFactor::Unscaled(linear_factor_resources),
                    }),
                );
            }
        } else {
            measurements.insert(selector, VariableResourceParams::Constant(resources));
        }
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
    for (syscall, resources) in measurements {
        raw_vc.os_resources.execute_syscalls.insert(syscall, resources);
    }
    expect_file![VersionedConstants::json_path(&version).unwrap()]
        .assert_eq(&raw_vc.to_string_pretty());
}

/// Measures the per-transaction-type overhead of `execute_transaction_inner` in the OS and
/// compares it against the versioned constants.
///
/// Methodology:
/// - Run a minimal transaction of the given type through the full OS.
/// - Compute: overhead = OS trace resources − blockifier business-logic resources
///   (execute_call_info + validate_call_info).
/// - The remainder is the pure OS scaffolding cost stored under `execute_txs_inner`.
#[tokio::test]
async fn test_execute_txs_inner_resources() {
    let version = StarknetVersion::LATEST;
    let mut raw_vc: RawVersionedConstants =
        serde_json::from_str(VersionedConstants::json_str(&version).unwrap()).unwrap();
    const N_TXS: usize = 7;

    let OsResourcesTestSetup {
        stable_contract_address,
        stable_contract_class_hash,
        mut test_builder,
        ..
    } = setup_test_builder().await;

    // Prepare the deploy account txs in advance, so we can fund the address before moving to the
    // next block (just so the funding tx is not in our measurement block). Use the stable contract
    // to prevent noise from changing contract address.
    let deploy_tx_base = DeployAccountTransaction::create(
        deploy_account_tx(
            deploy_account_tx_args! {
                class_hash: stable_contract_class_hash,
                resource_bounds: *NON_TRIVIAL_RESOURCE_BOUNDS,
                constructor_calldata: calldata![Felt::ZERO],
                // The stable contract was already deployed (from deployer address zero) with
                // trivial salt, so use non-trivial salt to get a new address.
                contract_address_salt: ContractAddressSalt(Felt::from(100)),
            },
            Nonce::default(),
        ),
        &test_builder.chain_id(),
    )
    .unwrap();
    test_builder.add_fund_address_tx_with_default_amount(deploy_tx_base.contract_address);
    let deploy_tx_extra = DeployAccountTransaction::create(
        deploy_account_tx(
            deploy_account_tx_args! {
                class_hash: stable_contract_class_hash,
                resource_bounds: *NON_TRIVIAL_RESOURCE_BOUNDS,
                constructor_calldata: calldata![Felt::ONE, Felt::ZERO],
                // The stable contract was already deployed (from deployer address zero) with
                // trivial salt, so use non-trivial salt to get a new address.
                contract_address_salt: ContractAddressSalt(Felt::from(100)),
            },
            Nonce::default(),
        ),
        &test_builder.chain_id(),
    )
    .unwrap();
    test_builder.add_fund_address_tx_with_default_amount(deploy_tx_extra.contract_address);
    test_builder.move_to_next_block();

    // Invoke.
    let invoke_args = invoke_tx_args! {
        sender_address: stable_contract_address,
        calldata: calldata![Felt::ZERO],
        resource_bounds: *NON_TRIVIAL_RESOURCE_BOUNDS,
        nonce: test_builder.next_nonce(stable_contract_address),
    };
    test_builder.add_invoke_tx(
        InvokeTransaction::create(invoke_tx(invoke_args), &test_builder.chain_id()).unwrap(),
        None,
        None,
    );

    // Invoke: one more calldata element.
    let invoke_args = invoke_tx_args! {
        sender_address: stable_contract_address,
        calldata: calldata![Felt::ONE, Felt::ZERO],
        resource_bounds: *NON_TRIVIAL_RESOURCE_BOUNDS,
        nonce: test_builder.next_nonce(stable_contract_address),
    };
    test_builder.add_invoke_tx(
        InvokeTransaction::create(invoke_tx(invoke_args), &test_builder.chain_id()).unwrap(),
        None,
        None,
    );

    // Declare. Choose a contract that is not edited or recompiled, to keep measurements stable.
    let declare_args = declare_tx_args! {
        sender_address: stable_contract_address,
        nonce: test_builder.next_nonce(stable_contract_address),
        resource_bounds: *NON_TRIVIAL_RESOURCE_BOUNDS,
    };
    test_builder.add_explicit_cairo1_declare_tx(
        &DATA_GAS_ACCOUNT_CONTRACT_SIERRA,
        (*DATA_GAS_ACCOUNT_CONTRACT_CASM).clone(),
        declare_args,
        &test_builder.chain_id(),
    );

    // Deploy account (pre-prepared).
    test_builder.add_deploy_account_tx(deploy_tx_base);
    test_builder.add_deploy_account_tx(deploy_tx_extra);

    // L1 handler.
    test_builder.add_l1_handler(
        stable_contract_address,
        "l1_handler",
        // From address, extra args length.
        calldata![Felt::from(100), Felt::ZERO],
        None,
    );
    test_builder.add_l1_handler(
        stable_contract_address,
        "l1_handler",
        // From address, extra args.
        calldata![Felt::from(100), Felt::ONE, Felt::ZERO],
        None,
    );

    // Execute the business logic and extract the business logic resources for each tx.
    let test_runner = test_builder.build().await;
    let business_logic_resources: [ExecutionResources; N_TXS] = test_runner
        .os_hints
        .os_input
        .os_block_inputs
        .last()
        .unwrap()
        .tx_execution_infos
        .iter()
        .map(|exec_info| {
            let mut business_logic_resources =
                [exec_info.execute_call_info.as_ref(), exec_info.validate_call_info.as_ref()]
                    .into_iter()
                    .flatten()
                    .map(|ci| ci.resources.vm_resources.clone())
                    .fold(ExecutionResources::default(), |acc, resources| &acc + &resources);
            // TODO(Dori): Consider supporting memory-hole counting in the OsLogger. Until then, we
            //   cannot subtract inner calls with positive memory-hole counts from the OsLogger
            //   resources.
            business_logic_resources.n_memory_holes = 0;
            business_logic_resources
        })
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();

    // Run the OS part.
    let test_output = test_runner.run();
    test_output.perform_default_validations();

    // Fetch the OS resources for each tx.
    let [
        invoke_base,
        invoke_extra,
        declare_overhead,
        deploy_account_base,
        deploy_account_extra,
        l1_handler_base,
        l1_handler_extra,
    ]: [ExecutionResources; N_TXS] = test_output
        .runner_output
        .txs_trace
        .iter()
        .rev()
        .take(N_TXS)
        .rev()
        .map(|trace| trace.get_resources().unwrap().clone())
        .zip(business_logic_resources)
        .map(|(os_resources, business_logic_resources)| {
            (&os_resources - &business_logic_resources).filter_unused_builtins()
        })
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();

    // Invoke: variable cost, with scaling of 2.
    let VariableResourceParams::WithFactor(mut invoke_resources_params) = raw_vc
        .os_resources
        .execute_txs_inner
        .get(&TransactionType::InvokeFunction)
        .unwrap()
        .clone()
    else {
        panic!(
            "Invoke resources params has unexpected structure: {:?}",
            raw_vc.os_resources.execute_txs_inner.get(&TransactionType::InvokeFunction).unwrap()
        );
    };
    let VariableCallDataFactor::Scaled(mut invoke_scaling_factor) =
        invoke_resources_params.calldata_factor
    else {
        panic!(
            "Invoke scaling factor has unexpected structure: {:?}",
            invoke_resources_params.calldata_factor
        );
    };
    assert_eq!(
        invoke_scaling_factor.scaling_factor, 2,
        "Invoke scaling factor has unexpected value: {:?}",
        invoke_scaling_factor.scaling_factor
    );
    invoke_scaling_factor.resources = (&invoke_extra - &invoke_base).filter_unused_builtins();
    invoke_resources_params.calldata_factor = VariableCallDataFactor::Scaled(invoke_scaling_factor);
    invoke_resources_params.constant = invoke_base;
    raw_vc.os_resources.execute_txs_inner.insert(
        TransactionType::InvokeFunction,
        VariableResourceParams::WithFactor(invoke_resources_params),
    );

    // Declare: constant cost.
    assert_matches!(
        raw_vc.os_resources.execute_txs_inner.get(&TransactionType::Declare).unwrap(),
        VariableResourceParams::Constant(_),
        "Declare resources params has unexpected structure: {:?}",
        raw_vc.os_resources.execute_txs_inner.get(&TransactionType::Declare).unwrap()
    );
    raw_vc
        .os_resources
        .execute_txs_inner
        .insert(TransactionType::Declare, VariableResourceParams::Constant(declare_overhead));

    // Deploy account: variable cost, with scaling of 2.
    let VariableResourceParams::WithFactor(mut deploy_account_resources_params) =
        raw_vc.os_resources.execute_txs_inner.get(&TransactionType::DeployAccount).unwrap().clone()
    else {
        panic!(
            "Deploy account resources params has unexpected structure: {:?}",
            raw_vc.os_resources.execute_txs_inner.get(&TransactionType::DeployAccount).unwrap()
        );
    };
    let VariableCallDataFactor::Scaled(mut deploy_account_scaling_factor) =
        deploy_account_resources_params.calldata_factor
    else {
        panic!(
            "Deploy account scaling factor has unexpected structure: {:?}",
            deploy_account_resources_params.calldata_factor
        );
    };
    assert_eq!(
        deploy_account_scaling_factor.scaling_factor, 2,
        "Deploy account scaling factor has unexpected value: {:?}",
        deploy_account_scaling_factor.scaling_factor
    );
    deploy_account_scaling_factor.resources =
        (&deploy_account_extra - &deploy_account_base).filter_unused_builtins();
    deploy_account_resources_params.calldata_factor =
        VariableCallDataFactor::Scaled(deploy_account_scaling_factor);
    deploy_account_resources_params.constant = deploy_account_base;
    raw_vc.os_resources.execute_txs_inner.insert(
        TransactionType::DeployAccount,
        VariableResourceParams::WithFactor(deploy_account_resources_params),
    );

    // L1 handler: variable cost, unscaled.
    let VariableResourceParams::WithFactor(mut l1_handler_resources_params) =
        raw_vc.os_resources.execute_txs_inner.get(&TransactionType::L1Handler).unwrap().clone()
    else {
        panic!(
            "L1 handler resources params has unexpected structure: {:?}",
            raw_vc.os_resources.execute_txs_inner.get(&TransactionType::L1Handler).unwrap()
        );
    };
    assert_matches!(
        l1_handler_resources_params.calldata_factor,
        VariableCallDataFactor::Unscaled(_),
        "L1 handler scaling factor has unexpected structure: {:?}",
        l1_handler_resources_params.calldata_factor
    );
    l1_handler_resources_params.calldata_factor = VariableCallDataFactor::Unscaled(
        (&l1_handler_extra - &l1_handler_base).filter_unused_builtins(),
    );
    l1_handler_resources_params.constant = l1_handler_base;
    raw_vc.os_resources.execute_txs_inner.insert(
        TransactionType::L1Handler,
        VariableResourceParams::WithFactor(l1_handler_resources_params),
    );

    // Verify computation.
    expect_file![VersionedConstants::json_path(&version).unwrap()]
        .assert_eq(&raw_vc.to_string_pretty());
}
