use std::collections::HashMap;

use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use ethnum::U256;
use num_bigint::{BigInt, BigUint, RandBigInt, RandomBits, Sign, ToBigInt};
use rand::Rng;
use starknet_os::hints::hint_implementation::kzg::utils::{split_bigint3, BASE, BLS_PRIME};
use starknet_os::test_utils::cairo_runner::{
    run_cairo_0_entry_point,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    PointerArg,
    ValueArg,
};
use starknet_types_core::felt::Felt;
use tracing::info;

use crate::os_cli::tests::types::{OsPythonTestResult, OsSpecificTestError};
use crate::os_cli::tests::utils::{
    pack_bigint3,
    seeded_random_prng,
    test_cairo_function,
    DEFAULT_PRIME,
};
use crate::shared_utils::types::PythonTestError;

// TODO(Amos): This test is incomplete. Add the rest of the test cases and remove this todo.
pub(crate) fn test_bls_field(input: &str) -> OsPythonTestResult {
    test_bigint3_to_uint256(input)?;
    test_felt_to_bigint3(input)?;
    test_horner_eval(input)?;
    // TODO(Amos): Uncomment once WRITE_DIVMOD_SEGMENT cairo-vm implementation is fixed (and
    // accepts negative values).
    // test_reduced_mul_random(input)?;
    Ok("".to_string())
}

fn get_entrypoint_runner_config() -> EntryPointRunnerConfig {
    EntryPointRunnerConfig {
        layout: LayoutName::small,
        add_main_prefix_to_entrypoint: false,
        ..Default::default()
    }
}

fn test_bigint3_to_uint256(input: &str) -> OsPythonTestResult {
    let mut rng = seeded_random_prng();
    let random_u256_big_uint: BigUint = rng.sample(RandomBits::new(256));
    let random_u256_bigint = BigInt::from_biguint(Sign::Plus, random_u256_big_uint);
    info!("random 256 bit bigint in `test_bigint3_to_uint256`: {random_u256_bigint}");
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
        input,
        "starkware.starknet.core.os.data_availability.bls_field.bigint3_to_uint256",
        &explicit_args,
        &implicit_args,
        &expected_explicit_args,
        &expected_implicit_args,
        HashMap::new(),
    )?;
    Ok("".to_string())
}

fn test_felt_to_bigint3(input: &str) -> OsPythonTestResult {
    let values: [BigInt; 9] = [
        0.into(),
        1.into(),
        DEFAULT_PRIME.clone() - 1,
        DEFAULT_PRIME.clone() - 2,
        BASE.clone() - 1,
        BASE.clone(),
        BASE.pow(2_u32) - 1,
        BASE.pow(2_u32),
        DEFAULT_PRIME.clone() / 2,
    ];
    let entrypoint_runner_config = get_entrypoint_runner_config();
    for value in values {
        let explicit_args: [EndpointArg; 1] = [Felt::from(value.clone()).into()];
        let implicit_args = [ImplicitArg::Builtin(BuiltinName::range_check)];

        let split_value = split_bigint3(value.clone()).unwrap();
        let expected_explicit_args = [EndpointArg::Value(ValueArg::Array(split_value.to_vec()))];
        let n_range_checks = if value == DEFAULT_PRIME.clone() - 1 { 0 } else { 6 };
        let expected_implicit_args: [EndpointArg; 1] = [n_range_checks.into()];

        test_cairo_function(
            &entrypoint_runner_config,
            input,
            "starkware.starknet.core.os.data_availability.bls_field.felt_to_bigint3",
            &explicit_args,
            &implicit_args,
            &expected_explicit_args,
            &expected_implicit_args,
            HashMap::new(),
        )?;
    }
    Ok("".to_string())
}

fn test_horner_eval(input: &str) -> OsPythonTestResult {
    let mut rng = seeded_random_prng();
    let entrypoint_runner_config = get_entrypoint_runner_config();

    for n_coefficients in [0, 100, 4096] {
        info!("Testing horner_eval with {n_coefficients} coefficients.");
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

        let (_, explicit_retdata) = run_cairo_0_entry_point(
            &entrypoint_runner_config,
            input,
            "starkware.starknet.core.os.data_availability.bls_field.horner_eval",
            &explicit_args,
            &implicit_args,
            &[EndpointArg::Value(ValueArg::Array(vec![Felt::ZERO, Felt::ZERO, Felt::ZERO]))],
            HashMap::new(),
        )
        .map_err(|error| {
            PythonTestError::SpecificError(OsSpecificTestError::Cairo0EntryPointRunner(error))
        })?;

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
            % BLS_PRIME.clone();

        // Calculate expected result.
        info!("Calculating expected result.");
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

    Ok("".to_string())
}

#[allow(dead_code)]
fn test_reduced_mul_random(input: &str) -> OsPythonTestResult {
    // Generate a,b in (-limb_limit, limb_limit).
    let mut rng = seeded_random_prng();
    let limb_limit = 2_i128.pow(104);
    let a_split =
        (0..3).map(|_| rng.gen_range(-limb_limit + 1..limb_limit).into()).collect::<Vec<Felt>>();
    let b_split =
        (0..3).map(|_| rng.gen_range(-limb_limit + 1..limb_limit).into()).collect::<Vec<Felt>>();

    let explicit_args = [
        EndpointArg::Value(ValueArg::Array(a_split.clone())),
        EndpointArg::Value(ValueArg::Array(b_split.clone())),
    ];
    let implicit_args = [ImplicitArg::Builtin(BuiltinName::range_check)];
    let expected_implicit_args: [EndpointArg; 1] = [11.into()];
    let expected_result = split_bigint3(
        (pack_bigint3(&a_split) * pack_bigint3(&b_split)) % BLS_PRIME.to_bigint().unwrap(),
    )
    .unwrap();
    let expected_explicit_args = [EndpointArg::Value(ValueArg::Array(expected_result.to_vec()))];
    test_cairo_function(
        &get_entrypoint_runner_config(),
        input,
        "starkware.starknet.core.os.data_availability.bls_field.reduced_mul",
        &explicit_args,
        &implicit_args,
        &expected_explicit_args,
        &expected_implicit_args,
        HashMap::new(),
    )?;

    Ok("".to_string())
}
