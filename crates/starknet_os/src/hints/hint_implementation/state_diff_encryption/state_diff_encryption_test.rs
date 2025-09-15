use std::collections::HashMap;

use apollo_starknet_os_program::OS_PROGRAM_BYTES;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use itertools::Itertools;
use rstest::rstest;
use starknet_types_core::felt::Felt;

use crate::hints::hint_implementation::state_diff_encryption::utils::{
    compute_public_keys,
    decrypt_state_diff,
    encrypt_symmetric_key,
};
use crate::test_utils::cairo_runner::{
    initialize_cairo_runner,
    run_cairo_0_entrypoint,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    ValueArg,
};

#[rstest]
#[case::single_key(&[Felt::from(1234567890)])]
#[case::multiple_keys(&[Felt::from(123), Felt::from(456), Felt::from(789), Felt::from(101112)])]
fn test_state_diff_encryption_function(#[case] private_keys: &[Felt]) {
    // Set up committee keys for encryption/decryption.
    let public_keys: Vec<Felt> = compute_public_keys(private_keys);

    // Set up symmetric key and starknet private and public keys.
    let symmetric_key = Felt::from(987654321);
    let sn_private_keys: Vec<Felt> = private_keys
        .iter()
        .map(|&private_key| {
            private_key + Felt::from(1000) // simple transformation for testing
        })
        .collect();
    let sn_public_keys = compute_public_keys(&sn_private_keys);

    // Compute the encrypted symmetric keys.
    let encrypted_symmetric_keys =
        encrypt_symmetric_key(&sn_private_keys, &public_keys, symmetric_key);

    // Set up the entry point runner configuration.
    let runner_config = EntryPointRunnerConfig {
        layout: LayoutName::small,
        add_main_prefix_to_entrypoint: false,
        ..Default::default()
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
        EndpointArg::Value(ValueArg::Single(data_start.into())),
        EndpointArg::Value(ValueArg::Single(data_end.into())),
        EndpointArg::Value(ValueArg::Single(symmetric_key.into())),
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

    assert_eq!(implicit_return_values.len(), 2);
    let EndpointArg::Value(ValueArg::Single(MaybeRelocatable::RelocatableValue(encrypted_dst_end))) =
        implicit_return_values[1]
    else {
        panic!("Unexpected implicit return value");
    };

    let encrypted_dst_length = (encrypted_dst_end - encrypted_dst).unwrap();
    assert_eq!(data.len(), encrypted_dst_length);

    let encrypted_data = runner.vm.get_integer_range(encrypted_dst, encrypted_dst_length).unwrap();
    let encrypted_data: Vec<Felt> = encrypted_data.into_iter().map(|felt| *felt).collect();

    for ((&private_key, &sn_public_key), &encrypted_symmetric_key) in
        private_keys.iter().zip_eq(sn_public_keys.iter()).zip_eq(encrypted_symmetric_keys.iter())
    {
        let decrypted_data = decrypt_state_diff(
            private_key,
            sn_public_key,
            encrypted_symmetric_key,
            &encrypted_data,
        );
        assert_eq!(decrypted_data, data);
    }
}

#[rstest]
#[case::single_key(&[Felt::from(1234567890)])]
#[case::multiple_keys(&[Felt::from(123), Felt::from(456), Felt::from(789), Felt::from(101112)])]
fn test_compute_public_keys_function(#[case] private_keys: &[Felt]) {
    // Set up starknet private keys.
    let sn_private_keys_vector: Vec<Felt> = private_keys
        .iter()
        .map(|&private_key| {
            private_key + Felt::from(1000) // simple transformation for testing
        })
        .collect();

    // Set up the entry point runner configuration.
    let runner_config = EntryPointRunnerConfig {
        layout: LayoutName::starknet,
        add_main_prefix_to_entrypoint: false,
        ..Default::default()
    };

    let mut implicit_args = vec![
        ImplicitArg::Builtin(BuiltinName::range_check),
        ImplicitArg::Builtin(BuiltinName::ec_op),
    ];

    let entrypoint = "starkware.starknet.core.os.output.compute_public_keys";

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

    let sn_private_keys = runner.vm.add_memory_segment();
    runner
        .vm
        .load_data(
            sn_private_keys,
            &sn_private_keys_vector
                .clone()
                .into_iter()
                .map(Into::into)
                .collect::<Vec<MaybeRelocatable>>(),
        )
        .unwrap();

    let explicit_args = vec![
        EndpointArg::Value(ValueArg::Single(sn_private_keys_vector.len().into())),
        EndpointArg::Value(ValueArg::Single(sn_private_keys.into())),
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

    // [range_check_ptr, ec_op_ptr, encrypted_dst_end]
    assert_eq!(implicit_return_values.len(), 3);
    let EndpointArg::Value(ValueArg::Single(MaybeRelocatable::RelocatableValue(encrypted_dst_end))) =
        implicit_return_values[2]
    else {
        panic!("Unexpected implicit return value");
    };

    let encrypted_dst_length = (encrypted_dst_end - encrypted_dst).unwrap();
    assert_eq!(sn_private_keys_vector.len(), encrypted_dst_length);

    let sn_private_keys_from_memory =
        runner.vm.get_integer_range(encrypted_dst, encrypted_dst_length).unwrap();
    let sn_private_keys_from_memory: Vec<Felt> =
        sn_private_keys_from_memory.into_iter().map(|felt| *felt).collect();

    let expected_public_keys = compute_public_keys(&sn_private_keys_vector);
    assert_eq!(sn_private_keys_from_memory, expected_public_keys);
}
