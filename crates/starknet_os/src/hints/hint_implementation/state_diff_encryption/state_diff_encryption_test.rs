use std::collections::HashMap;

use apollo_starknet_os_program::OS_PROGRAM_BYTES;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::runners::cairo_runner::CairoRunner;
use itertools::Itertools;
use num_bigint::BigUint;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rstest::rstest;
use starknet_types_core::felt::Felt;

use crate::hints::hint_implementation::state_diff_encryption::utils::{
    compute_public_keys,
    decrypt_state_diff,
    decrypt_symmetric_key,
    encrypt_state_diff,
    encrypt_symmetric_key,
    maybe_decrypt_iter,
};
use crate::test_utils::cairo_runner::{
    initialize_cairo_runner,
    run_cairo_0_entrypoint,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    ValueArg,
};

fn add_memory_segment_and_load_explicit_arg(
    runner: &mut CairoRunner,
    data: &[Felt],
) -> (Relocatable, Relocatable) {
    let start = runner.vm.add_memory_segment();
    let end = runner
        .vm
        .load_data(start, &data.iter().map(Into::into).collect::<Vec<MaybeRelocatable>>())
        .unwrap();
    (start, end)
}

fn generate_committee_private_keys_and_symmetric_key(
    seed: u64,
    num_keys: usize,
) -> (Vec<Felt>, Felt) {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut get_random_nonzero_felt =
        || Felt::from(rng.gen_range(BigUint::new(vec![1])..Felt::prime()));

    let private_keys = (0..num_keys).map(|_| get_random_nonzero_felt()).collect();

    let symmetric_key = get_random_nonzero_felt();

    (private_keys, symmetric_key)
}

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
    let (data_start, data_end) = add_memory_segment_and_load_explicit_arg(&mut runner, &data);

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

    let entrypoint = "starkware.starknet.core.os.encrypt.compute_public_keys";

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

    let (sn_private_keys, _) =
        add_memory_segment_and_load_explicit_arg(&mut runner, &sn_private_keys_vector);

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

    let actual_public_keys_length = (encrypted_dst_end - encrypted_dst).unwrap();
    assert_eq!(sn_private_keys_vector.len(), actual_public_keys_length);

    let sn_public_keys_from_memory: Vec<Felt> = runner
        .vm
        .get_integer_range(encrypted_dst, actual_public_keys_length)
        .unwrap()
        .into_iter()
        .map(|felt| *felt)
        .collect();

    let expected_public_keys = compute_public_keys(&sn_private_keys_vector);
    assert_eq!(sn_public_keys_from_memory, expected_public_keys);
}

#[rstest]
#[case::single_key(&[Felt::from(1234567890)])]
#[case::multiple_keys(&[Felt::from(123), Felt::from(456), Felt::from(789), Felt::from(101112)])]
fn test_symmetric_key_encryption_function(#[case] private_keys: &[Felt]) {
    // Set up committee keys for encryption/decryption.
    let public_keys_vector: Vec<Felt> = compute_public_keys(private_keys);

    // Set up symmetric key and starknet private and public keys.
    let symmetric_key = Felt::from(987654321);
    let sn_private_keys_vector: Vec<Felt> = private_keys
        .iter()
        .map(|&private_key| {
            private_key + Felt::from(1000) // simple transformation for testing
        })
        .collect();
    let sn_public_keys = compute_public_keys(&sn_private_keys_vector);

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

    let entrypoint = "starkware.starknet.core.os.encrypt.encrypt_symmetric_key";

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

    let (sn_private_keys, _) =
        add_memory_segment_and_load_explicit_arg(&mut runner, &sn_private_keys_vector);

    let (public_keys, _) =
        add_memory_segment_and_load_explicit_arg(&mut runner, &public_keys_vector);

    let explicit_args = vec![
        EndpointArg::Value(ValueArg::Single(sn_private_keys_vector.len().into())),
        EndpointArg::Value(ValueArg::Single(public_keys.into())),
        EndpointArg::Value(ValueArg::Single(sn_private_keys.into())),
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

    // [range_check_ptr, ec_op_ptr, encrypted_dst_end]
    assert_eq!(implicit_return_values.len(), 3);
    let EndpointArg::Value(ValueArg::Single(MaybeRelocatable::RelocatableValue(encrypted_dst_end))) =
        implicit_return_values[2]
    else {
        panic!("Unexpected implicit return value");
    };

    let actual_symmetric_keys_length = (encrypted_dst_end - encrypted_dst).unwrap();
    assert_eq!(public_keys_vector.len(), actual_symmetric_keys_length);

    let encrypted_symmetric_keys: Vec<Felt> = runner
        .vm
        .get_integer_range(encrypted_dst, actual_symmetric_keys_length)
        .unwrap()
        .into_iter()
        .map(|felt| *felt)
        .collect();

    // Compute the expected encrypted symmetric keys.
    let expected_encrypted_symmetric_keys =
        encrypt_symmetric_key(&sn_private_keys_vector, &public_keys_vector, symmetric_key);

    // Verify the encrypted symmetric keys match the expected values.
    assert_eq!(encrypted_symmetric_keys, expected_encrypted_symmetric_keys);

    // Decrypt the encrypted symmetric keys for each committee member with their parameters:
    // private_key and sn_public_key.
    // Verify that the decrypted symmetric key matches the original symmetric key.
    for ((&private_key, &sn_public_key), &encrypted_symmetric_key) in
        private_keys.iter().zip_eq(sn_public_keys.iter()).zip_eq(encrypted_symmetric_keys.iter())
    {
        let decrypted_symmetric_key =
            decrypt_symmetric_key(private_key, sn_public_key, encrypted_symmetric_key);
        assert_eq!(decrypted_symmetric_key, symmetric_key);
    }
}

/// Encrypt data, construct DA segment, and decrypt it. Make sure the result is identical to the
/// original data.
#[rstest]
#[case::single_member(43, Some(1))]
#[case::null_committee(457, None)]
#[case::empty_committee(458, Some(0))]
#[case::large_committee(790, Some(10))]
fn test_da_segment_encrypt_decrypt(
    #[case] seed: u64,
    #[case] num_committee_members: Option<usize>,
) {
    let original_data = vec![Felt::ZERO, Felt::ONE, Felt::TWO, Felt::THREE];

    // Generate committee private keys and symmetric key using randomness with the provided seed.
    let (private_keys, symmetric_key) =
        generate_committee_private_keys_and_symmetric_key(seed, num_committee_members.unwrap_or(0));

    // Set up committee keys for encryption/decryption.
    let public_keys_vector: Vec<Felt> = compute_public_keys(&private_keys);

    // Set up starknet private and public keys.
    let sn_private_keys_vector: Vec<Felt> =
        private_keys.iter().map(|&private_key| private_key + Felt::from(1000)).collect();
    let sn_public_keys = compute_public_keys(&sn_private_keys_vector);

    // Compute the encrypted symmetric keys.
    let encrypted_symmetric_keys =
        encrypt_symmetric_key(&sn_private_keys_vector, &public_keys_vector, symmetric_key);

    // Construct the DA segment.
    // If the committee is empty or null, the DA segment should be identical to the original data.
    // Otherwise, the DA segment should be encrypted.
    let da_segment = match num_committee_members {
        Some(n) if n > 0 => {
            let suffix = encrypt_state_diff(symmetric_key, &original_data);
            [vec![Felt::from(n)], sn_public_keys, encrypted_symmetric_keys, suffix].concat()
        }
        _ => original_data.clone(),
    };

    // Decrypt the DA segment.
    // Verify that the decrypted data matches the original data.
    let mut da_iter = da_segment.into_iter();
    let decrypted = maybe_decrypt_iter(
        &mut da_iter,
        if num_committee_members.is_some() { Some(&private_keys) } else { None },
    );
    assert_eq!(decrypted.collect::<Vec<Felt>>(), original_data);
}
