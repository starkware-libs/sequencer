use std::collections::HashMap;

use apollo_starknet_os_program::OS_PROGRAM_BYTES;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::runners::cairo_runner::CairoRunner;
use itertools::Itertools;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rstest::rstest;
use starknet_types_core::felt::Felt;

use crate::hints::hint_implementation::state_diff_encryption::utils::{
    compute_public_keys,
    decrypt_state_diff,
    decrypt_symmetric_key,
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

fn add_memory_segment_and_load_explicit_arg(
    runner: &mut CairoRunner,
    data: &[Felt],
) -> (Relocatable, Relocatable) {
    let start = runner.vm.add_memory_segment();

    let end = if data.is_empty() {
        // Handle empty data case - data_end equals data_start
        start
    } else {
        runner
            .vm
            .load_data(start, &data.iter().map(Into::into).collect::<Vec<MaybeRelocatable>>())
            .unwrap()
    };

    (start, end)
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
/// Tests encryption with various committee sizes and data configurations including:
/// - Single and multiple committee members
/// - Empty data, single elements, and large data arrays
/// - Different random seeds for deterministic key generation
///
/// ## Helper Functions
///
/// Generate committee private keys and symmetric key using a seeded random number generator.
fn generate_committee_private_keys_and_symmetric_key(
    seed: u64,
    num_keys: usize,
) -> (Vec<Felt>, Felt) {
    let mut rng = StdRng::seed_from_u64(seed);

    let private_keys = (0..num_keys)
        .map(|_| {
            // Generate a random u64 and convert to Felt, ensuring it's not zero
            let random_value = loop {
                let value = rng.gen::<u64>();
                if value != 0 {
                    break value;
                }
            };
            Felt::from(random_value)
        })
        .collect();

    // Generate symmetric key using the same seeded RNG
    let symmetric_key = loop {
        let value = rng.gen::<u64>();
        if value != 0 {
            break Felt::from(value);
        }
    };

    (private_keys, symmetric_key)
}

#[rstest]
// Single committee member with non-empty data
#[case::single_member_with_data(42, 1, vec![Felt::from(1), Felt::from(2), Felt::from(3)])]
// Multiple committee members with non-empty data
#[case::multiple_members_with_data(123, 4, vec![Felt::from(100), Felt::from(200)])]
// Single committee member with empty data
#[case::single_member_empty_data(456, 1, vec![])]
// Multiple committee members with empty data
#[case::multiple_members_empty_data(789, 3, vec![])]
// Single committee member with single element data
#[case::single_member_single_element(999, 1, vec![Felt::from(42)])]
// Multiple committee members with large data
#[case::multiple_members_large_data(111, 2, vec![Felt::from(1), Felt::from(2), Felt::from(3), Felt::from(4), Felt::from(5), Felt::from(6), Felt::from(7), Felt::from(8), Felt::from(9), Felt::from(10)])]
fn test_state_diff_encryption_function(
    #[case] seed: u64,
    #[case] num_committee_members: usize,
    #[case] data: Vec<Felt>,
) {
    // Generate committee private keys and symmetric key using randomness with the provided seed.
    let (private_keys, symmetric_key) =
        generate_committee_private_keys_and_symmetric_key(seed, num_committee_members);

    // Set up committee keys for encryption/decryption.
    let public_keys: Vec<Felt> = compute_public_keys(&private_keys);

    // Set up starknet private and public keys.
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

    // Use the parameterized data instead of hardcoded values
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
    // [range_check_ptr, encrypted_dst_end]
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

    // Only try to get encrypted data if there is data to encrypt
    let encrypted_data = if encrypted_dst_length > 0 {
        let encrypted_range =
            runner.vm.get_integer_range(encrypted_dst, encrypted_dst_length).unwrap();
        encrypted_range.into_iter().map(|felt| *felt).collect()
    } else {
        vec![]
    };

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
// Single committee member
#[case::single_member(42, 1)]
// Multiple committee members
#[case::multiple_members(123, 4)]
// Empty committee (edge case)
#[case::empty_committee(456, 0)]
// Large committee
#[case::large_committee(789, 10)]
fn test_compute_public_keys_function(#[case] seed: u64, #[case] num_committee_members: usize) {
    // Generate committee private keys using randomness with the provided seed.
    let (private_keys, _symmetric_key) =
        generate_committee_private_keys_and_symmetric_key(seed, num_committee_members);

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

    // Only try to get public keys if there are private keys
    let sn_public_keys_from_memory = if actual_public_keys_length > 0 {
        let public_keys_range =
            runner.vm.get_integer_range(encrypted_dst, actual_public_keys_length).unwrap();
        public_keys_range.into_iter().map(|felt| *felt).collect()
    } else {
        vec![]
    };

    let expected_public_keys = compute_public_keys(&sn_private_keys_vector);
    assert_eq!(sn_public_keys_from_memory, expected_public_keys);
}

#[rstest]
// Single committee member
#[case::single_member(42, 1)]
// Multiple committee members
#[case::multiple_members(123, 4)]
// Empty committee (edge case)
#[case::empty_committee(456, 0)]
// Large committee
#[case::large_committee(789, 10)]
fn test_symmetric_key_encryption_function(#[case] seed: u64, #[case] num_committee_members: usize) {
    // Generate committee private keys and symmetric key using randomness with the provided seed.
    let (private_keys, symmetric_key) =
        generate_committee_private_keys_and_symmetric_key(seed, num_committee_members);

    // Set up committee keys for encryption/decryption.
    let public_keys_vector: Vec<Felt> = compute_public_keys(&private_keys);

    // Set up starknet private and public keys.
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

    // Only try to get encrypted symmetric keys if there are keys to encrypt
    let encrypted_symmetric_keys = if actual_symmetric_keys_length > 0 {
        let encrypted_range =
            runner.vm.get_integer_range(encrypted_dst, actual_symmetric_keys_length).unwrap();
        encrypted_range.into_iter().map(|felt| *felt).collect()
    } else {
        vec![]
    };

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
