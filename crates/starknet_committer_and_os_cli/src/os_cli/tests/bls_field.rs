use std::collections::HashMap;

use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use ethnum::U256;
use num_bigint::{BigInt, BigUint, RandBigInt, RandomBits, Sign, ToBigInt};
use rand::Rng;
use starknet_os::hints::hint_implementation::kzg::utils::{split_bigint3, BASE, BLS_PRIME};
use starknet_os::test_utils::cairo_runner::{
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    PointerArg,
    ValueArg,
};
use starknet_types_core::felt::Felt;
use tracing::info;

use crate::os_cli::tests::types::OsPythonTestResult;
use crate::os_cli::tests::utils::{
    pack_bigint3,
    seeded_random_prng,
    test_cairo_function,
    DEFAULT_PRIME,
};

// TODO(Amos): This test is incomplete. Add the rest of the test cases and remove this todo.
pub(crate) fn test_bls_field(input: &str) -> OsPythonTestResult {
    test_bigint3_to_uint256(input)?;
    test_felt_to_bigint3(input)?;
    // TODO(Amos): uncomment once VM is upgraded to v2.0.0.
    // test_horner_eval(input)?;
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
    let expected_explicit_args: [EndpointArg; 2] = [low.into(), high.into()];
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
        let expected_explicit_args: Vec<EndpointArg> =
            split_value.iter().map(|&x| x.into()).collect::<Vec<EndpointArg>>();
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

#[allow(dead_code)]
fn test_horner_eval(input: &str) -> OsPythonTestResult {
    let mut rng = seeded_random_prng();
    let entrypoint_runner_config = get_entrypoint_runner_config();
    let mut n_coefficients_to_expected_result: HashMap<i32, [EndpointArg; 3]> = HashMap::new();
    n_coefficients_to_expected_result.insert(0_i32, [0.into(), 0.into(), 0.into()]);
    n_coefficients_to_expected_result.insert(
        100_i32,
        [
            30049504871598881073101566_u128.into(),
            65944866880181546056377353_u128.into(),
            8045773018308483897966805_u128.into(),
        ],
    );
    n_coefficients_to_expected_result.insert(
        4096_i32,
        [
            106435533756089035310503103_u128.into(),
            74211026253252940305985284_u128.into(),
            5800934992510396610932551_u128.into(),
        ],
    );

    for (n_coefficients, expected_result) in n_coefficients_to_expected_result.into_iter() {
        let mut explicit_args: Vec<EndpointArg> = vec![];
        let mut coefficients: Vec<Felt> = vec![];
        explicit_args.push(n_coefficients.into());
        for _ in 0..n_coefficients {
            let coefficient: Felt =
                RandBigInt::gen_bigint_range(&mut rng, &0.into(), &DEFAULT_PRIME).into();
            coefficients.push(coefficient);
        }
        explicit_args.push(EndpointArg::Pointer(PointerArg::Array(coefficients)));
        let point =
            RandBigInt::gen_bigint_range(&mut rng, &0.into(), &BLS_PRIME.to_bigint().unwrap());
        explicit_args
            .push(EndpointArg::Value(ValueArg::Array(split_bigint3(point).unwrap().into())));
        let implicit_args = [ImplicitArg::Builtin(BuiltinName::range_check)];
        // FIXME: Insert real builtin usage. note that this isn't checked in original test.
        let expected_implicit_args: [EndpointArg; 1] = [0.into()];
        test_cairo_function(
            &entrypoint_runner_config,
            input,
            "starkware.starknet.core.os.data_availability.bls_field.horner_eval",
            &explicit_args,
            &implicit_args,
            &expected_result,
            &expected_implicit_args,
            HashMap::new(),
        )?;
    }
    Ok("".to_string())
}

#[allow(dead_code)]
fn test_reduced_mul_random(input: &str) -> OsPythonTestResult {
    let mut rng = seeded_random_prng();
    let limb_limit = 2_i128.pow(104);
    let mut a_split = [Felt::from(0); 3];
    let mut b_split = [Felt::from(0); 3];
    for i in 0..3 {
        a_split[i] = rng.gen_range(-limb_limit..limb_limit).into();
        b_split[i] = rng.gen_range(-limb_limit..limb_limit).into();
    }

    let explicit_args = [
        EndpointArg::Value(ValueArg::Array(a_split.to_vec())),
        EndpointArg::Value(ValueArg::Array(a_split.to_vec())),
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
