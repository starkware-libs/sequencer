use std::collections::HashMap;
use std::sync::LazyLock;

use blockifier::blockifier_versioned_constants::{
    CallDataFactor, OsResources, ResourcesParams, VariableCallDataFactor,
};
use blockifier::execution::call_info::CallInfo;
use blockifier::execution::deprecated_syscalls::DeprecatedSyscallSelector;
use shared_execution_objects::central_objects::CentralTransactionExecutionInfo;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use starknet_api::executable_transaction::TransactionType;
use starknet_os::hint_processor::os_logger::OsTransactionTrace;

// Steps the OS spends returning from a non-call-contract syscall (overhead above the raw
// syscall resources measured in the trace).
pub(crate) const STEPS_FOR_RETURNING_FROM_SYSCALL: usize = 14;

// Steps the OS spends returning from a call-contract-like syscall, excluding the inner-call cost
// which is subtracted separately.
pub(crate) const STEPS_FOR_RETURNING_FROM_INNER_SYSCALL: usize = 8;

// Maximum step difference allowed between measured and stored values for syscall resources.
pub(crate) const SYSCALL_COMPARE_MARGIN: usize = 20;

// Maximum step difference allowed between measured and stored values for tx-type resources.
pub(crate) const TRANSACTION_COMPARE_MARGIN: usize = 60;

// Syscalls that make an inner call whose resources must be subtracted from the trace total.
pub(crate) const CALL_CONTRACT_SYSCALLS: &[DeprecatedSyscallSelector] =
    &[DeprecatedSyscallSelector::CallContract, DeprecatedSyscallSelector::LibraryCall];

/// Returns the number of times each syscall is expected to appear in the trace produced by
/// `OsResourcesTest.__execute__`. Used as a sanity check during extraction.
pub(crate) fn expected_syscall_call_counts() -> HashMap<DeprecatedSyscallSelector, usize> {
    use DeprecatedSyscallSelector::*;
    HashMap::from([
        (CallContract, 1),
        (LibraryCall, 1),
        (MetaTxV0, 1),
        (Deploy, 1),
        (EmitEvent, 1),
        (GetBlockHash, 1),
        (GetClassHashAt, 1),
        (GetExecutionInfo, 1),
        (Sha256ProcessBlock, 1),
        (ReplaceClass, 1),
        (SendMessageToL1, 1),
        (Secp256k1New, 2),
        (Secp256k1Add, 1),
        (Secp256k1GetPointFromX, 1),
        (Secp256k1GetXy, 1),
        (Secp256k1Mul, 1),
        (Secp256r1New, 2),
        (Secp256r1Add, 1),
        (Secp256r1GetPointFromX, 1),
        (Secp256r1GetXy, 1),
        (Secp256r1Mul, 1),
        (StorageRead, 1),
        (StorageWrite, 1),
    ])
}

/// Extracts per-syscall resource params from the OS trace of the `OsResourcesTest` invoke
/// transaction, mirroring the Python `get_execute_syscalls_resources_params` logic.
///
/// For call-contract-like syscalls (`CallContract`, `LibraryCall`) the inner-call vm resources
/// are subtracted and `STEPS_FOR_RETURNING_FROM_INNER_SYSCALL` is added back.
/// For all other syscalls `STEPS_FOR_RETURNING_FROM_SYSCALL` is added.
///
/// Hardcoded entries for Cairo 0-only syscalls (no longer exercised by the test contract) and
/// for `Keccak` / `KeccakRound` (not yet wired into the contract) are appended at the end.
pub(crate) fn extract_execute_syscalls_resources(
    tx_trace: &OsTransactionTrace,
    execute_call_info: &CallInfo,
) -> HashMap<DeprecatedSyscallSelector, ResourcesParams> {
    use DeprecatedSyscallSelector::*;

    let mut remaining_counts = expected_syscall_call_counts();
    let mut inner_call_idx: usize = 0;
    let mut syscall_params: HashMap<DeprecatedSyscallSelector, ExecutionResources> = HashMap::new();

    for syscall_trace in &tx_trace.syscalls {
        let selector = syscall_trace.selector;
        let count = remaining_counts
            .get_mut(&selector)
            .unwrap_or_else(|| panic!("Unexpected syscall in trace: {:?}", selector));
        assert!(*count > 0, "Syscall {:?} appeared more times than expected", selector);
        *count -= 1;

        let raw_resources = syscall_trace
            .resources
            .as_ref()
            .unwrap_or_else(|| panic!("Syscall {:?} has no resources", selector))
            .clone();

        let mut constant = if CALL_CONTRACT_SYSCALLS.contains(&selector) {
            let inner_vm = execute_call_info
                .inner_calls
                .get(inner_call_idx)
                .unwrap_or_else(|| {
                    panic!("No inner call at index {} for syscall {:?}", inner_call_idx, selector)
                })
                .resources
                .vm_resources
                .clone();
            inner_call_idx += 1;
            subtract_resources(&raw_resources, &inner_vm)
        } else {
            raw_resources
        };

        let steps_overhead = if CALL_CONTRACT_SYSCALLS.contains(&selector) {
            STEPS_FOR_RETURNING_FROM_INNER_SYSCALL
        } else {
            STEPS_FOR_RETURNING_FROM_SYSCALL
        };
        constant.n_steps += steps_overhead;
        constant.n_memory_holes = 0;

        syscall_params.insert(selector, constant);
    }

    // Verify all expected syscalls were seen.
    for (selector, remaining) in &remaining_counts {
        assert_eq!(
            *remaining, 0,
            "Syscall {:?} was expected but appeared fewer times than expected",
            selector
        );
    }

    // Hardcoded Cairo 0-only syscall costs (no longer executed by the test contract).
    // Values match the Python os_resources_test.py.
    insert_hardcoded(
        &mut syscall_params,
        DelegateCall,
        713,
        &[(BuiltinName::range_check, 19)],
    );
    insert_hardcoded(
        &mut syscall_params,
        DelegateL1Handler,
        692,
        &[(BuiltinName::range_check, 15)],
    );
    insert_hardcoded(&mut syscall_params, GetBlockNumber, 40, &[]);
    insert_hardcoded(&mut syscall_params, GetBlockTimestamp, 38, &[]);
    insert_hardcoded(&mut syscall_params, GetSequencerAddress, 34, &[]);
    insert_hardcoded(&mut syscall_params, GetTxSignature, 44, &[]);
    insert_hardcoded(
        &mut syscall_params,
        LibraryCallL1Handler,
        659,
        &[(BuiltinName::range_check, 15)],
    );

    // Cairo 0 execution-info aliases: same cost as GetExecutionInfo.
    let get_execution_info_resources = syscall_params[&GetExecutionInfo].clone();
    syscall_params.insert(GetCallerAddress, get_execution_info_resources.clone());
    syscall_params.insert(GetContractAddress, get_execution_info_resources.clone());
    syscall_params.insert(GetTxInfo, get_execution_info_resources);

    // Keccak and KeccakRound: hardcoded because the contract doesn't exercise them yet.
    insert_hardcoded(&mut syscall_params, Keccak, 100, &[]);
    insert_hardcoded(
        &mut syscall_params,
        KeccakRound,
        281,
        &[
            (BuiltinName::bitwise, 6),
            (BuiltinName::keccak, 1),
            (BuiltinName::range_check, 56),
        ],
    );

    // Wrap everything in ResourcesParams with an empty calldata_factor.
    syscall_params
        .into_iter()
        .map(|(selector, constant)| {
            (selector, ResourcesParams { constant, calldata_factor: VariableCallDataFactor::default() })
        })
        .collect()
}

/// Compares actual measured `OsResources` against the expected values stored in
/// `VersionedConstants`, allowing `SYSCALL_COMPARE_MARGIN` steps of drift for syscalls
/// and `TRANSACTION_COMPARE_MARGIN` for tx types. Builtins must match exactly.
pub(crate) fn compare_os_resources(actual: &OsResources, expected: &OsResources) {
    // --- execute_syscalls ---
    let actual_syscall_keys: std::collections::HashSet<_> =
        actual.execute_syscalls.keys().collect();
    let expected_syscall_keys: std::collections::HashSet<_> =
        expected.execute_syscalls.keys().collect();
    assert_eq!(
        actual_syscall_keys, expected_syscall_keys,
        "execute_syscalls key mismatch: actual has {:?}, expected has {:?}",
        actual_syscall_keys.difference(&expected_syscall_keys).collect::<Vec<_>>(),
        expected_syscall_keys.difference(&actual_syscall_keys).collect::<Vec<_>>(),
    );

    for selector in actual_syscall_keys {
        let actual_res = &actual.execute_syscalls[selector].constant;
        let expected_res = &expected.execute_syscalls[selector].constant;
        compare_execution_resources(
            actual_res,
            expected_res,
            SYSCALL_COMPARE_MARGIN,
            &format!("execute_syscalls[{selector:?}]"),
        );
    }

    // --- execute_txs_inner ---
    let actual_tx_keys: std::collections::HashSet<_> = actual.execute_txs_inner.keys().collect();
    let expected_tx_keys: std::collections::HashSet<_> =
        expected.execute_txs_inner.keys().collect();
    assert_eq!(
        actual_tx_keys, expected_tx_keys,
        "execute_txs_inner key mismatch",
    );

    for tx_type in actual_tx_keys {
        let actual_res = &actual.execute_txs_inner[tx_type].constant;
        let expected_res = &expected.execute_txs_inner[tx_type].constant;
        compare_execution_resources(
            actual_res,
            expected_res,
            TRANSACTION_COMPARE_MARGIN,
            &format!("execute_txs_inner[{tx_type:?}]"),
        );
    }

    // --- compute_os_kzg_commitment_info ---
    assert_eq!(
        actual.compute_os_kzg_commitment_info, expected.compute_os_kzg_commitment_info,
        "compute_os_kzg_commitment_info mismatch",
    );
}

fn compare_execution_resources(
    actual: &ExecutionResources,
    expected: &ExecutionResources,
    step_margin: usize,
    label: &str,
) {
    let step_diff = actual.n_steps.abs_diff(expected.n_steps);
    assert!(
        step_diff <= step_margin,
        "{label}: n_steps diff {step_diff} exceeds margin {step_margin} \
         (actual={}, expected={})",
        actual.n_steps,
        expected.n_steps,
    );
    assert_eq!(
        actual.builtin_instance_counter, expected.builtin_instance_counter,
        "{label}: builtin_instance_counter mismatch \
         (actual={:?}, expected={:?})",
        actual.builtin_instance_counter, expected.builtin_instance_counter,
    );
}

fn subtract_resources(
    lhs: &ExecutionResources,
    rhs: &ExecutionResources,
) -> ExecutionResources {
    let mut result = lhs.clone();
    result.n_steps = result.n_steps.saturating_sub(rhs.n_steps);
    result.n_memory_holes = result.n_memory_holes.saturating_sub(rhs.n_memory_holes);
    for (builtin, count) in &rhs.builtin_instance_counter {
        let entry = result.builtin_instance_counter.entry(*builtin).or_insert(0);
        *entry = entry.saturating_sub(*count);
    }
    result
}

fn insert_hardcoded(
    map: &mut HashMap<DeprecatedSyscallSelector, ExecutionResources>,
    selector: DeprecatedSyscallSelector,
    n_steps: usize,
    builtins: &[(BuiltinName, usize)],
) {
    assert!(
        !map.contains_key(&selector),
        "Syscall {selector:?} was unexpectedly already in the map",
    );
    map.insert(
        selector,
        ExecutionResources {
            n_steps,
            n_memory_holes: 0,
            builtin_instance_counter: builtins.iter().copied().collect(),
        },
    );
}

// Hardcoded calldata factors per transaction type, matching the Python
// TX_TYPE_TO_CALLDATA_FACTOR constants in os_resources_test.py.
// Each factor represents the additional resources consumed per calldata element.
pub(crate) static TX_TYPE_CALLDATA_FACTORS: LazyLock<HashMap<TransactionType, VariableCallDataFactor>> =
    LazyLock::new(|| {
        use TransactionType::*;
        HashMap::from([
            (
                InvokeFunction,
                VariableCallDataFactor::Scaled(CallDataFactor {
                    resources: ExecutionResources {
                        n_steps: 11,
                        n_memory_holes: 0,
                        builtin_instance_counter: HashMap::from([(BuiltinName::poseidon, 1)]),
                    },
                    scaling_factor: 2,
                }),
            ),
            (Declare, VariableCallDataFactor::default()),
            (
                DeployAccount,
                VariableCallDataFactor::Scaled(CallDataFactor {
                    resources: ExecutionResources {
                        n_steps: 37,
                        n_memory_holes: 0,
                        builtin_instance_counter: HashMap::from([
                            (BuiltinName::poseidon, 1),
                            (BuiltinName::pedersen, 2),
                        ]),
                    },
                    scaling_factor: 2,
                }),
            ),
            (
                L1Handler,
                VariableCallDataFactor::Scaled(CallDataFactor {
                    resources: ExecutionResources {
                        n_steps: 13,
                        n_memory_holes: 0,
                        builtin_instance_counter: HashMap::from([(BuiltinName::pedersen, 1)]),
                    },
                    scaling_factor: 1,
                }),
            ),
        ])
    });

/// Extracts per-transaction-type resource params from the OS traces of the four type-measurement
/// transactions (INVOKE, DECLARE, DEPLOY_ACCOUNT, L1_HANDLER) run on `EmptyAccount`, mirroring
/// `get_execute_transactions_resources` in os_resources_test.py.
///
/// For each transaction the business-logic resources (execute + validate call infos) are
/// subtracted from the total trace resources to isolate the OS overhead.
/// The calldata factors come from `TX_TYPE_CALLDATA_FACTORS`.
pub(crate) fn extract_execute_txs_inner_resources(
    tx_measurements: &[(TransactionType, &CentralTransactionExecutionInfo, &OsTransactionTrace)],
) -> HashMap<TransactionType, ResourcesParams> {
    tx_measurements
        .iter()
        .map(|(tx_type, exec_info, tx_trace)| {
            let total = tx_trace
                .resources
                .as_ref()
                .unwrap_or_else(|| panic!("OsTransactionTrace for {tx_type:?} has no resources"))
                .clone();

            // Sum resources from call_info (execute) and validate_info.
            let mut business_logic = ExecutionResources::default();
            if let Some(call_info) = &exec_info.execute_call_info {
                business_logic =
                    add_resources(&business_logic, &call_info.resources.vm_resources);
            }
            if let Some(validate_info) = &exec_info.validate_call_info {
                business_logic =
                    add_resources(&business_logic, &validate_info.resources.vm_resources);
            }

            let mut constant = subtract_resources(&total, &business_logic);
            constant.n_memory_holes = 0;

            let calldata_factor = TX_TYPE_CALLDATA_FACTORS[tx_type].clone();
            (*tx_type, ResourcesParams { constant, calldata_factor })
        })
        .collect()
}

fn add_resources(lhs: &ExecutionResources, rhs: &ExecutionResources) -> ExecutionResources {
    let mut result = lhs.clone();
    result.n_steps += rhs.n_steps;
    result.n_memory_holes += rhs.n_memory_holes;
    for (builtin, count) in &rhs.builtin_instance_counter {
        *result.builtin_instance_counter.entry(*builtin).or_insert(0) += count;
    }
    result
}
