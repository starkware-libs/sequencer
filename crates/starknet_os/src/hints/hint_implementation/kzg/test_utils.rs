<<<<<<< HEAD
use std::collections::{BTreeMap, HashMap};

use apollo_starknet_os_program::OS_PROGRAM_BYTES;
use blockifier::blockifier_versioned_constants::VersionedConstants;
use blockifier::execution::execution_utils::poseidon_hash_many_cost;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use cairo_vm::vm::runners::cairo_runner::{CairoRunner, ExecutionResources};
use num_bigint::RandBigInt;
use rand::rngs::SmallRng;
use rand::SeedableRng;
use starknet_types_core::felt::Felt;

use crate::hint_processor::common_hint_processor::CommonHintProcessor;
use crate::hints::hint_implementation::kzg::utils::FIELD_ELEMENTS_PER_BLOB;
use crate::test_utils::cairo_runner::{
    initialize_cairo_runner,
    run_cairo_0_entrypoint,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    PointerArg,
    ValueArg,
};

/// Returns an estimation of the resources required for `compute_os_kzg_commitment_info`.
pub fn estimate_os_kzg_commitment_computation_resources(
    vc: &VersionedConstants,
    state_diff_size: usize,
) -> ExecutionResources {
    let n_blobs = state_diff_size.div_ceil(FIELD_ELEMENTS_PER_BLOB);
    assert!(n_blobs > 0, "Number of blobs must be positive.");
    let mut resources = ExecutionResources {
        n_steps: vc.os_resources.compute_os_kzg_commitment_info.n_steps * state_diff_size
            + 214
            + (n_blobs - 1) * 138,
        n_memory_holes: 0,
        builtin_instance_counter: BTreeMap::from([
            (BuiltinName::poseidon, 1),
            (
                BuiltinName::range_check,
                24 + (n_blobs - 1) * 16
                    + state_diff_size
                        * vc.os_resources
                            .compute_os_kzg_commitment_info
                            .builtin_instance_counter
                            .get(&BuiltinName::range_check)
                            .unwrap_or(&0),
            ),
        ]),
    };
    resources += &poseidon_hash_many_cost(n_blobs * 2);
    resources += &poseidon_hash_many_cost(state_diff_size);
    resources
}

/// Runs the `compute_os_kzg_commitment_info` entrypoint and returns the runner and the DA segment.
pub fn run_compute_os_kzg_commitment_info(n: usize) -> (CairoRunner, Option<Vec<Felt>>) {
    let runner_config = EntryPointRunnerConfig {
        layout: LayoutName::all_cairo,
        add_main_prefix_to_entrypoint: false,
        trace_enabled: true,
        verify_secure: false,
        proof_mode: false,
    };
    let implicit_args = vec![
        ImplicitArg::Builtin(BuiltinName::range_check),
        ImplicitArg::Builtin(BuiltinName::poseidon),
    ];
    let (mut runner, program, entrypoint) = initialize_cairo_runner(
        &runner_config,
        OS_PROGRAM_BYTES,
        "starkware.starknet.core.os.data_availability.commitment.compute_os_kzg_commitment_info",
        &implicit_args,
        HashMap::new(),
    )
    .unwrap();

    // Create n random felt values.
    let mut rng = SmallRng::seed_from_u64(0);
    let prime = Felt::MAX.to_biguint() + 1u8;
    let data = (0..n)
        .map(|_| MaybeRelocatable::Int(Felt::from(rng.gen_biguint_below(&prime))))
        .collect::<Vec<_>>();
    let MaybeRelocatable::RelocatableValue(start_ptr) = runner.vm.gen_arg(&data).unwrap() else {
        panic!("Failed to generate start pointer");
    };
    let end_ptr = (start_ptr + data.len()).unwrap();

    // Prepare inputs and run.
    let explicit_args = vec![
        EndpointArg::Value(ValueArg::Single(start_ptr.into())),
        EndpointArg::Value(ValueArg::Single(end_ptr.into())),
    ];
    let expected_return_values = vec![EndpointArg::Pointer(PointerArg::Composed(vec![]))];
    let state_reader = None;
    let (_, _, mut hint_processor) = run_cairo_0_entrypoint(
        entrypoint,
        &explicit_args,
        &implicit_args,
        state_reader,
        &mut runner,
        &program,
        &runner_config,
        &expected_return_values,
    )
    .unwrap();

    (runner, hint_processor.get_da_segment().clone())
}
||||||| c96dea6126
=======
use num_bigint::BigUint;

pub(crate) fn horner_eval(coefficients: &[BigUint], point: &BigUint, prime: &BigUint) -> BigUint {
    coefficients.iter().rev().fold(BigUint::ZERO, |acc, coeff| (acc * point + coeff) % prime)
}
>>>>>>> origin/main-v0.14.1-committer
