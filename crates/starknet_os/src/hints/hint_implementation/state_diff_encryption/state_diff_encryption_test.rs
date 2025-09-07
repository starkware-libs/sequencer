use std::collections::HashMap;

use apollo_starknet_os_program::OS_PROGRAM_BYTES;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use starknet_types_core::curve::AffinePoint;
use starknet_types_core::felt::Felt;

use crate::hints::hint_implementation::state_diff_encryption::utils::decrypt_state_diff;
use crate::test_utils::cairo_runner::{
    initialize_cairo_runner,
    run_cairo_0_entrypoint,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    ValueArg,
};

#[test]
fn test_state_diff_encryption_decryption() {
    // Set up keys for encryption/decryption.
    let private_key = Felt::from(1234567890u64);
    let public_key_point: AffinePoint = AffinePoint::generator() * private_key;
    let public_keys = vec![public_key_point.x()];
    let n_keys = 1;

    // Set up the entry point runner configuration.
    let runner_config = EntryPointRunnerConfig {
        layout: LayoutName::all_cairo,
        trace_enabled: false,
        verify_secure: false,
        proof_mode: false,
        add_main_prefix_to_entrypoint: false, // Set to false since we're using full path.
    };

    let mut implicit_args = vec![ImplicitArg::Builtin(BuiltinName::range_check)];

    let entrypoint = "starkware.starknet.core.os.output.encrypt";

    let (mut runner, program, entrypoint) = initialize_cairo_runner(
        &runner_config,
        OS_PROGRAM_BYTES,
        entrypoint,
        &implicit_args,
        HashMap::new(),
    )
    .unwrap();

    let encrypted_dst = runner.vm.add_memory_segment();
    implicit_args
        .push(ImplicitArg::NonBuiltin(EndpointArg::Value(ValueArg::Single(encrypted_dst.into()))));

    let public_keys_ptr = runner.vm.add_memory_segment();
    let _public_keys_end = runner
        .vm
        .load_data(
            public_keys_ptr,
            &public_keys
                .into_iter()
                .map(Into::into)
                .collect::<Vec<MaybeRelocatable>>(),
        )
        .unwrap();

    let data = vec![Felt::from(1), Felt::from(2), Felt::from(3)];
    let data_start = runner.vm.add_memory_segment();
    let data_end = runner
        .vm
        .load_data(data_start, &data.into_iter().map(Into::into).collect::<Vec<MaybeRelocatable>>())
        .unwrap();

    let explicit_args = vec![
        EndpointArg::Value(ValueArg::Single(n_keys.into())),
        EndpointArg::Value(ValueArg::Single(public_keys_ptr.into())),
        EndpointArg::Value(ValueArg::Single(data_start.into())),
        EndpointArg::Value(ValueArg::Single(data_end.into())),
    ];
    let state_reader = None;
    let expected_explicit_return_values: Vec<EndpointArg> = vec![];
    let (implicit_return_values, _explicit_return_values) = run_cairo_0_entrypoint(
        entrypoint,
        &explicit_args,
        &implicit_args,
        state_reader,
        &mut runner,
        &program,
        &runner_config,
        &expected_explicit_return_values,
    )
    .unwrap();

    let EndpointArg::Value(ValueArg::Single(MaybeRelocatable::Int(encrypted_dst_end))) =
        implicit_return_values[0]
    else {
        panic!("Unexpected implicit return value");
    };

    let encrypted_dst_size = encrypted_dst_end - encrypted_dst;

    let encrypted_data = runner.vm.get_integer_range(encrypted_dst, encrypted_dst_size).unwrap();
    let encrypted_data: Vec<Felt> = encrypted_data.into_iter().map(|felt| *felt).collect();
    // FIXME: Do we also put n_keys in encrypted_data?
    let sn_public_key = encrypted_data[0];
    let encrypted_symmetric_key = encrypted_data[1];
    let encrypted_state_diff = &encrypted_data[2..];

    let decrypted_data = decrypt_state_diff(
        private_key,
        sn_public_key,
        encrypted_symmetric_key,
        encrypted_state_diff,
    );
    assert_eq!(decrypted_data, data);
}
