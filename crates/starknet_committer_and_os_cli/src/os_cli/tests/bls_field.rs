use std::collections::HashMap;

use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use ethnum::U256;
use num_bigint::{BigInt, BigUint, RandomBits, Sign};
use rand::Rng;
use starknet_os::hints::hint_implementation::kzg::utils::{split_bigint3, BASE};
use starknet_os::test_utils::cairo_runner::{
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    ValueArg,
};
use starknet_types_core::felt::Felt;
use tracing::info;

use crate::os_cli::tests::types::OsPythonTestResult;
use crate::os_cli::tests::utils::{seeded_random_pnrg, test_cairo_function, DEFAULT_PRIME};

// TODO(Amos): This test is incomplete. Add the rest of the test cases and remove this todo.
pub(crate) fn test_bls_field(input: &str) -> OsPythonTestResult {
    test_bigint3_to_uint256(input)?;
    test_felt_to_bigint3(input)?;
    Ok("".to_string())
}

fn test_bigint3_to_uint256(input: &str) -> OsPythonTestResult {
    let mut rng = seeded_random_pnrg();
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

    let entrypoint_runner_config = EntryPointRunnerConfig {
        layout: LayoutName::small,
        add_main_prefix_to_entrypoint: false,
        ..Default::default()
    };
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
    let entrypoint_runner_config = EntryPointRunnerConfig {
        layout: LayoutName::small,
        add_main_prefix_to_entrypoint: false,
        ..Default::default()
    };
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
