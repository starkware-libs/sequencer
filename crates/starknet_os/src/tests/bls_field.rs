use std::array;
use std::collections::HashMap;

use apollo_starknet_os_program::OS_PROGRAM_BYTES;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::program::Program;
use ethnum::U256;
use num_bigint::{BigInt, BigUint, RandBigInt, RandomBits, Sign, ToBigInt};
use num_integer::Integer;
use rand::Rng;
use rstest::rstest;
use starknet_types_core::felt::Felt;

use crate::hints::hint_implementation::kzg::utils::{split_bigint3, BASE, BLS_PRIME};
use crate::test_utils::cairo_runner::{
    run_cairo_0_entry_point,
    EndpointArg,
    ImplicitArg,
    PointerArg,
    ValueArg,
};
use crate::test_utils::utils::{
    get_entrypoint_runner_config,
    pack_bigint3,
    seeded_random_prng,
    test_cairo_function,
    DEFAULT_PRIME,
};

const REDUCED_MUL_LIMB_BOUND: i128 = 2_i128.pow(104);

// TODO(Nimrod): Move this next to the BLS hints implementation.

fn run_reduced_mul_test(a_split: &[Felt], b_split: &[Felt]) {
    let explicit_args = [
        EndpointArg::Value(ValueArg::Array(a_split.to_vec())),
        EndpointArg::Value(ValueArg::Array(b_split.to_vec())),
    ];
    let implicit_args = [ImplicitArg::Builtin(BuiltinName::range_check)];
    let expected_implicit_args: [EndpointArg; 1] = [11.into()];
    let expected_result = split_bigint3(
        (pack_bigint3(a_split) * pack_bigint3(b_split)).mod_floor(&BLS_PRIME.to_bigint().unwrap()),
    )
    .unwrap();
    let expected_explicit_args = [EndpointArg::Value(ValueArg::Array(expected_result.to_vec()))];
    test_cairo_function(
        &get_entrypoint_runner_config(),
        OS_PROGRAM_BYTES,
        "starkware.starknet.core.os.data_availability.bls_field.reduced_mul",
        &explicit_args,
        &implicit_args,
        &expected_explicit_args,
        &expected_implicit_args,
        HashMap::new(),
    );
}

#[test]
fn test_bigint3_to_uint256() {
    let mut rng = seeded_random_prng();
    let random_u256_big_uint: BigUint = rng.sample(RandomBits::new(256));
    let random_u256_bigint = BigInt::from_biguint(Sign::Plus, random_u256_big_uint);
    let cairo_bigin3 = EndpointArg::Value(ValueArg::Array(
        split_bigint3(random_u256_bigint.clone()).unwrap().to_vec(),
    ));
    let explicit_args = [cairo_bigin3];
    let implicit_args = [ImplicitArg::Builtin(BuiltinName::range_check)];

    let two_to_128 = BigInt::from_bytes_be(Sign::Plus, &U256::from(2_u32).pow(128).to_be_bytes());
    let low = Felt::from(random_u256_bigint.clone() % two_to_128);
    let high = Felt::from(random_u256_bigint >> 128);
    let expected_explicit_args = [EndpointArg::Value(ValueArg::Array(vec![low, high]))];
    let expected_implicit_args: [EndpointArg; 1] = [4.into()];

    let entrypoint_runner_config = get_entrypoint_runner_config();
    test_cairo_function(
        &entrypoint_runner_config,
        OS_PROGRAM_BYTES,
        "starkware.starknet.core.os.data_availability.bls_field.bigint3_to_uint256",
        &explicit_args,
        &implicit_args,
        &expected_explicit_args,
        &expected_implicit_args,
        HashMap::new(),
    );
}

#[rstest]
fn test_felt_to_bigint3(
    #[values(
    0.into(),
    1.into(),
    DEFAULT_PRIME.clone() - 1,
    DEFAULT_PRIME.clone() - 2,
    BASE.clone() - 1,
    BASE.clone(),
    BASE.pow(2_u32) - 1,
    BASE.pow(2_u32),
    DEFAULT_PRIME.clone() / 2
)]
    value: BigInt,
) {
    let entrypoint_runner_config = get_entrypoint_runner_config();

    let explicit_args: [EndpointArg; 1] = [Felt::from(value.clone()).into()];
    let implicit_args = [ImplicitArg::Builtin(BuiltinName::range_check)];

    let split_value = split_bigint3(value.clone()).unwrap();
    let expected_explicit_args = [EndpointArg::Value(ValueArg::Array(split_value.to_vec()))];
    let n_range_checks = if value == DEFAULT_PRIME.clone() - 1 { 0 } else { 6 };
    let expected_implicit_args: [EndpointArg; 1] = [n_range_checks.into()];

    test_cairo_function(
        &entrypoint_runner_config,
        OS_PROGRAM_BYTES,
        "starkware.starknet.core.os.data_availability.bls_field.felt_to_bigint3",
        &explicit_args,
        &implicit_args,
        &expected_explicit_args,
        &expected_implicit_args,
        HashMap::new(),
    );
}

#[test]
fn test_horner_eval() {
    let mut rng = seeded_random_prng();
    let entrypoint_runner_config = get_entrypoint_runner_config();

    for n_coefficients in [0, 100, 4096] {
        let mut explicit_args: Vec<EndpointArg> = vec![];
        explicit_args.push(n_coefficients.into());
        let coefficients: Vec<Felt> = (0..n_coefficients)
            .map(|_| Felt::from(RandBigInt::gen_bigint_range(&mut rng, &0.into(), &DEFAULT_PRIME)))
            .collect();

        explicit_args.push(EndpointArg::Pointer(PointerArg::Array(coefficients.clone())));
        let point =
            RandBigInt::gen_bigint_range(&mut rng, &0.into(), &BLS_PRIME.to_bigint().unwrap());
        explicit_args.push(EndpointArg::Value(ValueArg::Array(
            split_bigint3(point.clone()).unwrap().into(),
        )));
        let implicit_args = [ImplicitArg::Builtin(BuiltinName::range_check)];

        let (_, explicit_retdata, _) = run_cairo_0_entry_point(
            &entrypoint_runner_config,
            OS_PROGRAM_BYTES,
            "starkware.starknet.core.os.data_availability.bls_field.horner_eval",
            &explicit_args,
            &implicit_args,
            &[EndpointArg::Value(ValueArg::Array(vec![Felt::ZERO, Felt::ZERO, Felt::ZERO]))],
            HashMap::new(),
            None,
        )
        .unwrap();

        // Get actual result.
        assert_eq!(
            explicit_retdata.len(),
            1,
            "Expected 1 explicit return value, got {}",
            explicit_retdata.len()
        );
        let split_actual_result = if let EndpointArg::Value(ValueArg::Array(result_coefficients)) =
            explicit_retdata.first().unwrap()
        {
            assert_eq!(
                result_coefficients.len(),
                3,
                "expected 3 coefficients in result, got {}",
                result_coefficients.len()
            );
            result_coefficients
        } else {
            panic!(
                "Unexpected result type. Expected `EndpointArg::Value(ValueArg::Array(_))`, got \
                 {:?}",
                explicit_retdata.first().unwrap()
            );
        };
        let actual_result = (BigUint::from_bytes_be(&split_actual_result[0].to_bytes_be())
            + BigUint::from_bytes_be(&split_actual_result[1].to_bytes_be())
                * BASE.to_biguint().unwrap()
            + BigUint::from_bytes_be(&split_actual_result[2].to_bytes_be())
                * BASE.to_biguint().unwrap().pow(2))
        .mod_floor(&BLS_PRIME.clone());

        // Calculate expected result.
        let expected_result =
            coefficients.iter().enumerate().fold(BigUint::ZERO, |acc, (i, coefficient)| {
                acc + BigUint::from_bytes_be(&coefficient.to_bytes_be())
                    * point.to_biguint().unwrap().modpow(&BigUint::from(i), &BLS_PRIME.clone())
            }) % BLS_PRIME.clone();

        assert_eq!(
            actual_result, expected_result,
            "expected result does not match actual result. Actual result: {actual_result}, \
             Expected result: {expected_result}"
        );
    }
}

#[test]
fn test_reduced_mul_random() {
    // Generate a,b in (-REDUCED_MUL_LIMB_LIMIT, REDUCED_MUL_LIMB_LIMIT).
    let mut rng = seeded_random_prng();
    let a_split = (0..3)
        .map(|_| rng.gen_range(-REDUCED_MUL_LIMB_BOUND + 1..REDUCED_MUL_LIMB_BOUND).into())
        .collect::<Vec<Felt>>();
    let b_split = (0..3)
        .map(|_| rng.gen_range(-REDUCED_MUL_LIMB_BOUND + 1..REDUCED_MUL_LIMB_BOUND).into())
        .collect::<Vec<Felt>>();

    run_reduced_mul_test(&a_split, &b_split)
}

#[test]
fn test_reduced_mul_parameterized() {
    let max_value = Felt::from(REDUCED_MUL_LIMB_BOUND - 1);
    let min_value = Felt::from(-REDUCED_MUL_LIMB_BOUND + 1);
    let values: [([Felt; 3], [Felt; 3]); 4] = [
        (array::from_fn(|_| max_value), array::from_fn(|_| max_value)),
        (array::from_fn(|_| min_value), array::from_fn(|_| min_value)),
        ([-Felt::ONE, Felt::ZERO, Felt::ZERO], [Felt::ONE, Felt::ZERO, Felt::ZERO]),
        ([Felt::ONE, Felt::from(2), Felt::from(3)], [Felt::ZERO, Felt::ZERO, Felt::ZERO]),
    ];
    for (a_split, b_split) in values {
        run_reduced_mul_test(&a_split, &b_split);
    }
}

#[test]
fn test_bls_prime_value() {
    let entrypoint = None;
    let program = Program::from_bytes(OS_PROGRAM_BYTES, entrypoint).unwrap();
    let actual_split_bls_prime: [Felt; 3] = array::from_fn(|i| {
        *program
            .constants
            .get(&format!("starkware.starknet.core.os.data_availability.bls_field.P{}", i))
            .unwrap()
    });
    let expected_split_bls_prime = split_bigint3(BLS_PRIME.to_bigint().unwrap()).unwrap();
    assert_eq!(
        expected_split_bls_prime, actual_split_bls_prime,
        "Expected BLS prime value to be {expected_split_bls_prime:?}, got \
         {actual_split_bls_prime:?}"
    );
}
