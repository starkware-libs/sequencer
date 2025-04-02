use std::collections::HashMap;

use cairo_vm::types::builtin_name::BuiltinName;
use ethnum::{u256, U256};
use num_bigint::{BigInt, BigUint, Sign};
use starknet_os::hints::shared_utils::split_bigint3;
use starknet_os::test_utils::cairo_runner::{EndpointArg, ImplicitArg, ValueArg};
use starknet_types_core::felt::Felt;

use crate::os_cli::tests::types::OsPythonTestResult;
use crate::os_cli::tests::utils::{get_random_u256, test_cairo_function};

// TODO(Amos): This test is incomplete. Add the rest of the test cases and remove this todo.
pub(crate) fn test_bls_field(input: &str) -> OsPythonTestResult {
    test_bigint3_to_uint256(input)?;
    Ok("".to_string())
}

fn test_bigint3_to_uint256(input: &str) -> OsPythonTestResult {
    // FIXME: range should include u256::MAX.
    let random_u256 = get_random_u256(&mut rand::thread_rng(), u256::from(0_u32), u256::MAX);
    let random_u256_bigint = BigInt::from(BigUint::from_bytes_be(&random_u256.to_be_bytes()));
    let cairo_bigin3 = EndpointArg::Value(ValueArg::Array(
        split_bigint3(random_u256_bigint.clone()).unwrap().to_vec(),
    ));
    let explicit_args = [cairo_bigin3];
    let implicit_args = [ImplicitArg::Builtin(BuiltinName::range_check)];

    let two_to_128 = BigInt::from_bytes_be(Sign::Plus, &U256::from(2_u32).pow(128).to_be_bytes());
    let low = Felt::from(random_u256_bigint.clone() % two_to_128);
    let high = Felt::from(random_u256_bigint >> 128);
    let expceted_explicit_args: [EndpointArg; 2] = [low.into(), high.into()];
    let expected_implicit_args: [EndpointArg; 1] = [4.into()];

    test_cairo_function(
        input,
        "bigint3_to_uint256",
        &explicit_args,
        &implicit_args,
        &expceted_explicit_args,
        &expected_implicit_args,
        HashMap::new(),
    )?;
    Ok("".to_string())
}
