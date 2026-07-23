use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock};

use blockifier::blockifier_versioned_constants::{
    CallDataFactor,
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
use blockifier::transaction::objects::ExecutionResourcesTraits;
use blockifier_test_utils::cairo_versions::RunnableCairo1;
use blockifier_test_utils::contracts::FeatureContract;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use expect_test::expect_file;
use indexmap::IndexMap;
use shared_execution_objects::central_objects::CentralTransactionExecutionInfo;
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
use starknet_api::transaction::fields::{Calldata, ContractAddressSalt};
use starknet_api::transaction::{L2ToL1Payload, MessageToL1};
use starknet_api::versioned_constants_logic::VersionedConstantsTrait;
use starknet_api::{calldata, declare_tx_args, deploy_account_tx_args, invoke_tx_args};
use starknet_os::hint_processor::os_logger::ResourceFinalizer;
use starknet_os::test_utils::{
    SHA256_BATCH_RESOURCES_LINEAR_UNSCALED,
    SHA256_BATCH_SIZE,
    SHA512_BATCH_RESOURCES_LINEAR_UNSCALED,
    SHA512_BATCH_SIZE,
};
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
const UNMEASURABLE_SYSCALLS: [Selector; 12] = [
    Selector::DelegateCall,
    Selector::DelegateL1Handler,
    Selector::GetBlockNumber,
    Selector::GetBlockTimestamp,
    Selector::GetCallerAddress,
    Selector::GetContractAddress,
    Selector::GetSequencerAddress,
    Selector::GetTxInfo,
    Selector::GetTxSignature,
    Selector::LibraryCallL1Handler,
    Selector::StorageRead,
    Selector::StorageWrite,
];

/// Store a mapping from a linearly-charged syscall, with the number of "linear elements" in it's
/// first measurement. For example, if we measure the base and linear costs of a [Selector::Deploy]
/// by measuring:
/// ```
/// let deploy_from_zero = true;
/// let M1 = COST(deploy_syscall(class_hash, salt, calldata: [0], deploy_from_zero));
/// let M2 = COST(deploy_syscall(class_hash, salt, calldata: [100] + [1; 100], deploy_from_zero));
/// ```
/// then the base and linear parts can be computed by:
/// ```
/// let LINEAR = (M2 - M1) / 100;
/// let BASE = M1 - LINEAR;
/// ```
/// The formula for `LINEAR` is simple, and note that we must subtract one `LINEAR` from `M1` to get
/// `BASE` because the `M1` measurement has a single calldata element (length of calldata: 0).
/// Note: Keccak does not store the linear factor in the same entry in the versioned constants, but
/// it does have a measurable linear factor stored under [Selector::KeccakRound].
static SYSCALLS_WITH_LINEAR_FACTOR: LazyLock<HashMap<Selector, usize>> = LazyLock::new(|| {
    HashMap::from([
        (Selector::Deploy, 1),
        (Selector::Keccak, 0),
        (Selector::MetaTxV0, 1),
        (Selector::SendMessageToL1, 0),
    ])
});
const LARGE_INPUT_LENGTH: usize = 100;

/// Syscalls that are implemented using virtual builtins. Such syscalls have their "heavy lifting"
/// executed after the execute_syscalls part of the OS, so the consumed resources are not captured
/// by the OsLogger.
const SYSCALLS_WITH_VIRTUAL_BUILTINS: [Selector; 2] =
    [Selector::Sha256ProcessBlock, Selector::Sha512ProcessBlock];

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
        Selector::Sha256ProcessBlock => (&measured_base
            + &SHA256_BATCH_RESOURCES_LINEAR_UNSCALED.div_ceil(SHA256_BATCH_SIZE))
            .filter_unused_builtins(),
        Selector::Sha512ProcessBlock => (&measured_base
            + &SHA512_BATCH_RESOURCES_LINEAR_UNSCALED.div_ceil(SHA512_BATCH_SIZE))
            .filter_unused_builtins(),
        _ => panic!("Resource update not implemented for virtual builtin syscall: {selector:?}."),
    }
}

/// Setup an test builder with
/// 1. the OS-resources contract deployed,
/// 2. funded (it is an account contract as well),
/// 3. the initial block context (and therefore subsequent block contexts) with the minimal sierra
///    version for gas tracking set to "infinity", in order to force step tracking mode, and
/// 4. the stable contract declared and deployed.
///
/// Pass the raw VC used in tests, to make sure the test initial state is set up consistently.
async fn setup_test_builder(raw_vc: Option<&RawVersionedConstants>) -> OsResourcesTestSetup {
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
        if let Some(raw_vc) = raw_vc {
            assert_eq!(vc, raw_vc.clone().into());
        }
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

/// Utility method to create dummy calldata to a cairo Span argument.
fn span_calldata(n_elements: usize) -> Calldata {
    let mut calldata = vec![Felt::from(n_elements)];
    calldata.extend(vec![Felt::ZERO; n_elements]);
    Calldata(Arc::new(calldata))
}

/// Regression test for the list of syscalls called during the fee transfer phase of a transaction.
#[tokio::test]
async fn test_fee_transfer_syscalls() {
    let OsResourcesTestSetup { os_resources_contract_address, test_builder: mut builder, .. } =
        setup_test_builder(None).await;

    // Invoke from the OS resources contract, with zeros as calldata, to make the __execute__ do
    // nothing. All resulting events should be from the fee transfer call.
    builder.add_invoke_tx(
        InvokeTransaction::create(
            invoke_tx(invoke_tx_args! {
                sender_address: os_resources_contract_address,
                calldata: calldata![Felt::ZERO, Felt::ZERO],
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
    } = setup_test_builder(Some(&raw_vc)).await;

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

    // Add the expected messages to L1.
    test_builder.messages_to_l1.push(MessageToL1 {
        from_address: os_resources_contract_address,
        to_address: EthAddress::try_from(Felt::from(100)).unwrap(),
        payload: L2ToL1Payload(vec![]),
    });
    test_builder.messages_to_l1.push(MessageToL1 {
        from_address: os_resources_contract_address,
        to_address: EthAddress::try_from(Felt::from(100)).unwrap(),
        payload: L2ToL1Payload(vec![Felt::ONE; LARGE_INPUT_LENGTH]),
    });

    // Run test. Grab the execution info from the runner (for later) before consuming it.
    let test_runner = test_builder.build().await;
    let inner_calls = test_runner
        .os_hints
        .last_block_input()
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
    let mut second_visit_syscalls = HashSet::new();
    let mut measurements: IndexMap<Selector, VariableResourceParams> = IndexMap::new();
    // If the syscall incurs an inner call, subtract the inner call overhead.
    let mut maybe_deduct_inner =
        |total: ExecutionResources, selector: Selector| -> ExecutionResources {
            // We assume no inner calls have nested inner calls (all inner calls are leaves).
            (if selector.is_calling_syscall() {
                // TODO(Dori): Take opcodes (like blake) into account, instead of using the
                // vm_resources field.
                // TODO(Dori): Consider supporting memory-hole counting in the OsLogger. Until then,
                //   we cannot subtract inner calls with positive memory-hole counts from the
                //   OsLogger resources.
                let mut to_deduct = inner_calls_iter.next().unwrap().resources.vm_resources;
                to_deduct.n_memory_holes = 0;
                &total - &to_deduct
            } else {
                total
            })
            .filter_unused_builtins()
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
        let mut resources =
            maybe_deduct_inner(syscall_trace.get_resources().unwrap().clone(), selector);

        // Virtual builtins require adjustment.
        if SYSCALLS_WITH_VIRTUAL_BUILTINS.contains(&selector) {
            resources = update_resources_for_virtual_builtin_syscall(selector, resources);
        }

        // If this is a syscall with a linear factor, the next syscall should be an invocation of
        // the same syscall with +1 linear element.
        if let Some(linear_count_in_base) = SYSCALLS_WITH_LINEAR_FACTOR.get(&selector) {
            let next_syscall_trace = syscalls_iter.next().unwrap();
            assert_eq!(
                selector,
                next_syscall_trace.get_selector(),
                "Expected next syscall to be the same as the current syscall {selector:?}, but \
                 got {:?}.",
                next_syscall_trace.get_selector()
            );
            let next_resources =
                maybe_deduct_inner(next_syscall_trace.get_resources().unwrap().clone(), selector);

            // Keccak is a special case:
            // 1. We store the linear cost as a separate syscall.
            // 2. Currently, the Keccak base cost is enforced in the OS to equal the syscall base
            //    cost (`static_assert KECCAK_GAS_COST == SYSCALL_BASE_GAS_COST`).
            // TODO(Dori): If and when (2) is no longer the case, no need to replace `resources`
            //   (measured keccak base cost) with the syscall base cost, and no need to recompute
            //   the linear factor.
            if selector == Selector::Keccak {
                let RawStepGasCost { step_gas_cost: n_steps } =
                    raw_vc.os_constants.syscall_base_gas_cost.clone();
                let constant_resources = ExecutionResources {
                    n_steps: n_steps.0.try_into().unwrap(),
                    ..Default::default()
                };
                let linear_factor_resources = (&next_resources - &constant_resources)
                    .div_ceil(LARGE_INPUT_LENGTH)
                    .filter_unused_builtins();
                measurements
                    .insert(Selector::Keccak, VariableResourceParams::Constant(constant_resources));
                measurements.insert(
                    Selector::KeccakRound,
                    VariableResourceParams::Constant(linear_factor_resources),
                );
            } else {
                let linear_factor_resources = (&next_resources - &resources)
                    .div_ceil(LARGE_INPUT_LENGTH)
                    .filter_unused_builtins();
                // Linear factor is computed; deduct the linear overhead from the base cost to get
                // the real base cost.
                let constant_resources = (&resources
                    - &(&linear_factor_resources * *linear_count_in_base))
                    .filter_unused_builtins();

                measurements.insert(
                    selector,
                    VariableResourceParams::WithFactor(ResourcesParams {
                        constant: constant_resources,
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

    // For linear factor measurements, it's not enough to just add one more calldata element; the
    // increase is not the same per element. The linear scale is on average.
    const NON_TRIVIAL_SCALING_FACTOR: usize = 2;
    const N_EXTRA_ARGS: usize = 10;

    let OsResourcesTestSetup {
        stable_contract_address,
        stable_contract_class_hash,
        mut test_builder,
        ..
    } = setup_test_builder(Some(&raw_vc)).await;

    // Prepare the deploy account txs in advance, so we can fund the address before moving to the
    // next block (just so the funding tx is not in our measurement block). Use the stable contract
    // to prevent noise from changing contract address.
    let deploy_tx_first = DeployAccountTransaction::create(
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
    test_builder.add_fund_address_tx_with_default_amount(deploy_tx_first.contract_address);
    let deploy_tx_second = DeployAccountTransaction::create(
        deploy_account_tx(
            deploy_account_tx_args! {
                class_hash: stable_contract_class_hash,
                resource_bounds: *NON_TRIVIAL_RESOURCE_BOUNDS,
                constructor_calldata: span_calldata(N_EXTRA_ARGS),
                // The stable contract was already deployed (from deployer address zero) with
                // trivial salt, so use non-trivial salt to get a new address.
                contract_address_salt: ContractAddressSalt(Felt::from(100)),
            },
            Nonce::default(),
        ),
        &test_builder.chain_id(),
    )
    .unwrap();
    test_builder.add_fund_address_tx_with_default_amount(deploy_tx_second.contract_address);
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
    // Invoke: scale-factor more calldata elements.
    let invoke_args = invoke_tx_args! {
        sender_address: stable_contract_address,
        calldata: span_calldata(N_EXTRA_ARGS),
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
    test_builder.add_deploy_account_tx(deploy_tx_first);
    test_builder.add_deploy_account_tx(deploy_tx_second);

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
        Calldata(Arc::new(
            [vec![Felt::from(100), Felt::from(N_EXTRA_ARGS)], vec![Felt::ZERO; N_EXTRA_ARGS]]
                .concat(),
        )),
        None,
    );

    // Execute the business logic and extract the business logic resources for each tx.
    let test_runner = test_builder.build().await;
    let business_logic_resources: [ExecutionResources; N_TXS] = test_runner
        .os_hints
        .last_block_input()
        .tx_execution_infos
        .iter()
        .map(
            |CentralTransactionExecutionInfo {
                 execute_call_info,
                 validate_call_info,
                 // Fee transfer resources are ignored when counting transaction-specific overhead.
                 fee_transfer_call_info: _,
                 ..
             }| {
                let mut business_logic_resources =
                    [execute_call_info.as_ref(), validate_call_info.as_ref()]
                        .into_iter()
                        .flatten()
                        .map(|ci| ci.resources.vm_resources.clone())
                        .fold(ExecutionResources::default(), |acc, resources| &acc + &resources);
                // TODO(Dori): Consider supporting memory-hole counting in the OsLogger. Until then,
                // we   cannot subtract inner calls with positive memory-hole counts
                // from the OsLogger   resources.
                business_logic_resources.n_memory_holes = 0;
                business_logic_resources
            },
        )
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();

    // Run the OS part.
    let test_output = test_runner.run();
    test_output.perform_default_validations();

    // Fetch the OS resources for each tx.
    let [
        invoke_first,
        invoke_second,
        declare_constant,
        deploy_first,
        deploy_second,
        l1_handler_first,
        l1_handler_second,
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

    // Update the VC with the measurements.
    // For transaction types with linear factors, the first call has one linear element (calldata
    // length, of zero), so one linear cost must be subtracted from the first measurement to get the
    // base cost.
    let compute_linear_factor =
        |first: &ExecutionResources, second: &ExecutionResources, scaling_factor: usize| {
            (&(second - first).filter_unused_builtins() * scaling_factor).div_ceil(N_EXTRA_ARGS)
        };
    let invoke_linear_factor =
        compute_linear_factor(&invoke_first, &invoke_second, NON_TRIVIAL_SCALING_FACTOR);
    let deploy_linear_factor =
        compute_linear_factor(&deploy_first, &deploy_second, NON_TRIVIAL_SCALING_FACTOR);
    // L1 handler linear factor is unscaled.
    let l1_handler_linear_factor = compute_linear_factor(&l1_handler_first, &l1_handler_second, 1);
    raw_vc.os_resources.execute_txs_inner.extend([
        (
            TransactionType::InvokeFunction,
            VariableResourceParams::WithFactor(ResourcesParams {
                constant: (&invoke_first - &invoke_linear_factor).filter_unused_builtins(),
                calldata_factor: VariableCallDataFactor::Scaled(CallDataFactor {
                    resources: invoke_linear_factor,
                    scaling_factor: NON_TRIVIAL_SCALING_FACTOR,
                }),
            }),
        ),
        (TransactionType::Declare, VariableResourceParams::Constant(declare_constant)),
        (
            TransactionType::DeployAccount,
            VariableResourceParams::WithFactor(ResourcesParams {
                constant: (&deploy_first - &deploy_linear_factor).filter_unused_builtins(),
                calldata_factor: VariableCallDataFactor::Scaled(CallDataFactor {
                    resources: deploy_linear_factor,
                    scaling_factor: NON_TRIVIAL_SCALING_FACTOR,
                }),
            }),
        ),
        (
            TransactionType::L1Handler,
            VariableResourceParams::WithFactor(ResourcesParams {
                constant: (&l1_handler_first - &l1_handler_linear_factor).filter_unused_builtins(),
                calldata_factor: VariableCallDataFactor::Unscaled(l1_handler_linear_factor),
            }),
        ),
    ]);

    // Verify computation.
    expect_file![VersionedConstants::json_path(&version).unwrap()]
        .assert_eq(&raw_vc.to_string_pretty());
}
