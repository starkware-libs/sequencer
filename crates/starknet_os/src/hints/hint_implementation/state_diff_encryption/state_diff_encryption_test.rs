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

/// Tests the state diff encryption functionality using the Cairo OS encrypt function.
///
/// # Test Flow
/// 1. **Setup**: Creates committee and StarkNet public/private key pairs and generates the
///    symmetric key.
/// 2. **Symmetric Key Encryption**: Encrypts the symmetric key using each committee member's public
///    key and starknet private keys.
/// 3. **Data Encryption**: Uses the Cairo OS `encrypt` function to encrypt test data with the
///    symmetric key.
/// 4. **Verification**: Decrypts the data using each committee member's private key and verifies
///    correctness.
///
/// # Encryption Algorithm
/// The Cairo `encrypt` function uses Blake2s hashing with the following scheme:
/// - For each data element at index `i`: `encrypted[i] = data[i] + Blake2s(symmetric_key || i)`
///
/// # Test Cases
/// - `single_key`: Tests encryption with a single committee member
/// - `multiple_keys`: Tests encryption with multiple committee members (distributed decryption
///   scenario).
///
/// # Parameters
/// - `private_keys`: Committee private keys used for symmetric key encryption/decryption.

// TODO(Yonatan): Use randomness with a seed to generate the committee and Starknet private keys.
// (parameterize over the number of the committee private keys).
// TODO(Yonatan): Parameterize over the data to encrypt (length and values, test empty data).
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

    let entrypoint = "starkware.starknet.core.os.encrypt.encrypt";

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
    let symmetric_key_segment = runner.vm.add_memory_segment();
    runner.vm.load_data(symmetric_key_segment, &vec![symmetric_key.into()]).unwrap();
    let explicit_args = vec![
        EndpointArg::Value(ValueArg::Single(data_start.into())),
        EndpointArg::Value(ValueArg::Single(data_end.into())),
        EndpointArg::Value(ValueArg::Single(symmetric_key_segment.into())),
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
    // [range_check_prt, encrypted_dst_end]
    assert_eq!(implicit_return_values.len(), 2);
    let EndpointArg::Value(ValueArg::Single(MaybeRelocatable::RelocatableValue(encrypted_dst_end))) =
        implicit_return_values[1]
    else {
        panic!(
            "Unexpected implicit return value for encrypted_dst_end got: {:?}",
            implicit_return_values[1]
        );
    };

    let encrypted_dst_length = (encrypted_dst_end - encrypted_dst).unwrap();
    assert_eq!(data.len(), encrypted_dst_length);

    let encrypted_data = runner.vm.get_integer_range(encrypted_dst, encrypted_dst_length).unwrap();
    let encrypted_data: Vec<Felt> = encrypted_data.into_iter().map(|felt| *felt).collect();

    // Decrypt the encrypted data for each committee member with their parameters:
    // private_key, sn_public_key, and their specific encrypted_symmetric_key.
    // Verify that the decrypted data matches the original data.
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
