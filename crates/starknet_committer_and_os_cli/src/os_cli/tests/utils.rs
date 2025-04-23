use std::any::Any;
use std::collections::HashMap;
use std::sync::LazyLock;

use ethnum::U256;
use num_bigint::{BigInt, Sign};
use rand::rngs::StdRng;
use rand::SeedableRng;
use starknet_os::hints::hint_implementation::kzg::utils::BASE;
use starknet_os::test_utils::cairo_runner::{EndpointArg, EntryPointRunnerConfig, ImplicitArg};
use starknet_os::test_utils::utils::run_cairo_function_and_check_result;
use starknet_types_core::felt::Felt;

use crate::os_cli::tests::types::{OsPythonTestResult, OsSpecificTestError};
use crate::shared_utils::types::PythonTestError;

// 2**251 + 17 * 2**192 + 1
pub static DEFAULT_PRIME: LazyLock<BigInt> = LazyLock::new(|| {
    BigInt::from_bytes_be(
        Sign::Plus,
        &(U256::from(2_u32).pow(251) + 17 * U256::from(2_u32).pow(192) + 1).to_be_bytes(),
    )
});

#[allow(clippy::too_many_arguments)]
pub(crate) fn test_cairo_function(
    runner_config: &EntryPointRunnerConfig,
    program_str: &str,
    function_name: &str,
    explicit_args: &[EndpointArg],
    implicit_args: &[ImplicitArg],
    expected_explicit_retdata: &[EndpointArg],
    expected_implicit_retdata: &[EndpointArg],
    hint_locals: HashMap<String, Box<dyn Any>>,
) -> OsPythonTestResult {
    run_cairo_function_and_check_result(
        runner_config,
        program_str,
        function_name,
        explicit_args,
        implicit_args,
        expected_explicit_retdata,
        expected_implicit_retdata,
        hint_locals,
    )
    .map_err(|error| {
        PythonTestError::SpecificError(OsSpecificTestError::Cairo0EntryPointRunner(error))
    })?;
    Ok("".to_string())
}

pub(crate) fn seeded_random_prng() -> StdRng {
    StdRng::seed_from_u64(42)
}

/// Returns the lift of the given field element, val, as a `BigInt` in the range
/// (-prime/2, prime/2).
fn as_int(val: &Felt, prime: &BigInt) -> BigInt {
    let val = val.to_bigint();
    if val < (prime / BigInt::from(2)) {
        return val.clone();
    }
    val - prime
}

/// Takes a BigInt3 struct represented by the limbs (d0, d1, d2) of
/// and reconstructs the corresponding integer (see split_bigint3()).
/// Note that the limbs do not have to be in the range [0, BASE).
/// Prime is used to handle negative values of the limbs.
pub fn pack_bigint3(limbs: &[Felt]) -> BigInt {
    assert!(limbs.len() == 3, "Expected 3 limbs, got {}", limbs.len());
    limbs.iter().enumerate().fold(BigInt::ZERO, |acc, (i, &limb)| {
        acc + as_int(&limb, &DEFAULT_PRIME) * BASE.pow(i.try_into().unwrap())
    })
}
