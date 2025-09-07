use std::collections::HashMap;

use apollo_starknet_os_program::OS_PROGRAM_BYTES;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use itertools::izip;
use rstest::rstest;
use starknet_types_core::curve::AffinePoint;
use starknet_types_core::felt::Felt;

use crate::hints::hint_implementation::state_diff_encryption::utils::decrypt_state_diff;
use crate::test_utils::cairo_runner::{
    initialize_cairo_runner,
    run_cairo_0_entrypoint,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    PointerArg,
    ValueArg,
};

#[rstest]
#[case::single_key(&[Felt::from(1234567890)])]
#[case::multiple_keys(&[Felt::from(123), Felt::from(456), Felt::from(789), Felt::from(101112)])]
fn test_state_diff_encryption_decryption(#[case] private_keys: &[Felt]) {
    // Set up keys for encryption/decryption.
    let n_keys = private_keys.len();
    let public_keys: Vec<Felt> = private_keys
        .iter()
        .map(|&private_key| {
            let public_key_point = &AffinePoint::generator() * private_key;
            public_key_point.x()
        })
        .collect();

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

    let data = vec![Felt::from(1), Felt::from(2), Felt::from(3)];
    let data_start = runner.vm.add_memory_segment();
    let data_end = runner
        .vm
        .load_data(
            data_start,
            &data.clone().into_iter().map(Into::into).collect::<Vec<MaybeRelocatable>>(),
        )
        .unwrap();

    let explicit_args = vec![
        EndpointArg::Value(ValueArg::Single(n_keys.into())),
        EndpointArg::Pointer(PointerArg::Array(
            public_keys.into_iter().map(Into::into).collect::<Vec<MaybeRelocatable>>(),
        )),
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

    let EndpointArg::Value(ValueArg::Single(MaybeRelocatable::RelocatableValue(encrypted_dst_end))) =
        implicit_return_values[0]
    else {
        panic!("Unexpected implicit return value");
    };

    let encrypted_dst_size = (encrypted_dst_end - encrypted_dst).unwrap();
    let expected_encrypted_dst_size = data.len() + 2 * n_keys;
    assert_eq!(encrypted_dst_size, expected_encrypted_dst_size);

    let encrypted_data = runner.vm.get_integer_range(encrypted_dst, encrypted_dst_size).unwrap();
    let encrypted_data: Vec<Felt> = encrypted_data.into_iter().map(|felt| *felt).collect();

    let n_keys = &encrypted_data[0].to_usize().unwrap();
    let sn_public_keys = &encrypted_data[1..n_keys + 1];
    let encrypted_symmetric_keys = &encrypted_data[n_keys + 1..2 * n_keys + 1];
    let encrypted_state_diff = &encrypted_data[2 * n_keys + 1..];

    for (&private_key, &sn_public_key, &encrypted_symmetric_key) in
        izip!(sn_public_keys.iter(), sn_public_keys.iter(), encrypted_symmetric_keys.iter())
    {
        let decrypted_data = decrypt_state_diff(
            private_key,
            sn_public_key,
            encrypted_symmetric_key,
            encrypted_state_diff,
        );
        assert_eq!(decrypted_data, data);
    }
}
